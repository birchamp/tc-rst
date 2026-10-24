//! Sync's git network calls: fetch, push and ls-remote, sent by libgit2 to a Door43
//! repository's smart HTTP address.
//!
//! Each call goes through the same gate as every HTTP request of this crate: it is refused
//! before anything is sent while the internet switch is off, and it uses the proxy that
//! [`Http::proxy_for`] names, never libgit2's own lookup, which knows no PAC script or WPAD
//! host. The switch is checked again in libgit2's callbacks, so a switch turned off during a
//! fetch stops it at the next chunk of the pack.
//!
//! A push that has started sending its pack cannot be stopped: git2 0.21's
//! `push_transfer_progress` and `pack_progress` callbacks cannot return a stop. The last
//! check before the pack leaves is in `push_negotiation`.
//!
//! libgit2 1.9.7 has no SOCKS proxy. A SOCKS proxy from the finder makes the call fail as
//! [`Door43Error::Unreachable`] with that reason.
//!
//! These calls block. The caller runs them on a blocking thread, with a repository that
//! `tc-store` opened.

use std::cell::{Cell, RefCell};

use git2::{Cred, ErrorClass, ErrorCode, FetchOptions, ProxyOptions, PushOptions, RemoteCallbacks};
use url::Url;

use crate::error::{AuthStatus, Door43Error, UnreachableCause};
use crate::http::Http;
use crate::names::{GitOid, RepoRef, ServerUrl};
use crate::push::refused_push;
use crate::secret::Session;

/// One Door43 repository as a git remote, and the account that signs in to it.
#[derive(Clone, Debug)]
pub struct GitRemote {
    pub server: ServerUrl,
    pub repo: RepoRef,
    /// `None` reads a public repository anonymously; a push needs an account.
    pub session: Option<Session>,
}

impl GitRemote {
    fn url(&self) -> Url {
        let name = format!("{}.git", self.repo.name);
        self.server
            .url_with_path([self.repo.owner.as_str(), name.as_str()])
    }

    fn route(&self, tail: &str) -> String {
        format!(
            "{} /{}.git/{tail}",
            if tail == "info/refs" { "GET" } else { "POST" },
            self.repo
        )
    }
}

/// Fetches `refspecs` from `remote` into `repo`. Tags are fetched only when a refspec names
/// them.
pub fn fetch(
    http: &Http,
    repo: &git2::Repository,
    remote: &GitRemote,
    refspecs: &[&str],
) -> Result<(), Door43Error> {
    let route = remote.route("git-upload-pack");
    let proxy = proxy(http, remote, &route)?;
    let mut callbacks = callbacks(http, remote);
    callbacks.transfer_progress(|_| http.internet.is_on());
    let mut options = FetchOptions::new();
    options
        .remote_callbacks(callbacks)
        .proxy_options(proxy)
        .download_tags(git2::AutotagOption::None);
    let mut git = repo
        .remote_anonymous(remote.url().as_str())
        .map_err(|e| failed(http, &route, e))?;
    git.fetch(refspecs, Some(&mut options), None)
        .map_err(|e| failed(http, &route, e))
}

/// Pushes `refspecs` of `repo` to `remote`. A ref Door43 refuses comes back as
/// [`refused_push`] reads it.
pub fn push(
    http: &Http,
    repo: &git2::Repository,
    remote: &GitRemote,
    refspecs: &[&str],
) -> Result<(), Door43Error> {
    let route = remote.route("git-receive-pack");
    let proxy = proxy(http, remote, &route)?;
    let refused = RefCell::new(None::<String>);
    let said = RefCell::new(String::new());
    let mut callbacks = callbacks(http, remote);
    callbacks.push_negotiation(|_| {
        if http.internet.is_on() {
            Ok(())
        } else {
            Err(git2::Error::from_str("the internet was turned off"))
        }
    });
    callbacks.push_update_reference(|_, status| {
        if let Some(status) = status {
            refused
                .borrow_mut()
                .get_or_insert_with(|| status.to_owned());
        }
        Ok(())
    });
    callbacks.sideband_progress(|text| {
        said.borrow_mut().push_str(&String::from_utf8_lossy(text));
        http.internet.is_on()
    });
    let mut options = PushOptions::new();
    options.remote_callbacks(callbacks).proxy_options(proxy);
    let mut git = repo
        .remote_anonymous(remote.url().as_str())
        .map_err(|e| failed(http, &route, e))?;
    git.push(refspecs, Some(&mut options))
        .map_err(|e| failed(http, &route, e))?;
    drop(options);
    match refused.into_inner() {
        Some(status) => Err(refused_push(&remote.repo, &status, &said.into_inner())),
        None => Ok(()),
    }
}

/// The refs `remote` advertises, by name.
pub fn ls_remote(
    http: &Http,
    repo: &git2::Repository,
    remote: &GitRemote,
) -> Result<Vec<(String, GitOid)>, Door43Error> {
    let route = remote.route("info/refs");
    let proxy = proxy(http, remote, &route)?;
    let mut git = repo
        .remote_anonymous(remote.url().as_str())
        .map_err(|e| failed(http, &route, e))?;
    let connection = git
        .connect_auth(
            git2::Direction::Fetch,
            Some(callbacks(http, remote)),
            Some(proxy),
        )
        .map_err(|e| failed(http, &route, e))?;
    let heads = connection.list().map_err(|e| failed(http, &route, e))?;
    heads
        .iter()
        .map(|head| {
            let oid = head.oid().to_string();
            let oid = oid.parse().map_err(|_| Door43Error::UnexpectedResponse {
                route: route.clone(),
                reason: format!("{oid:?} is not a commit sha"),
            })?;
            Ok((head.name().to_owned(), oid))
        })
        .collect()
}

/// The proxy options for `remote`, or the refusal while the internet is off.
fn proxy<'a>(
    http: &Http,
    remote: &GitRemote,
    route: &str,
) -> Result<ProxyOptions<'a>, Door43Error> {
    let offline = || Door43Error::Offline {
        route: route.to_owned(),
    };
    let mut options = ProxyOptions::new();
    if let Some(proxy) = http.proxy_for(&remote.url()).map_err(|_| offline())? {
        if proxy.scheme().starts_with("socks") {
            return Err(Door43Error::Unreachable {
                route: route.to_owned(),
                cause: UnreachableCause::Other(
                    "the proxy is a SOCKS proxy, which sync cannot use".to_owned(),
                ),
            });
        }
        options.url(proxy.as_str());
    }
    Ok(options)
}

/// The callbacks every call has: the account's token, offered once, and the switch checked
/// whenever the server speaks.
fn callbacks<'a>(http: &'a Http, remote: &'a GitRemote) -> RemoteCallbacks<'a> {
    let offered = Cell::new(false);
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |_, _, _| {
        if !http.internet.is_on() {
            return Err(git2::Error::from_str("the internet was turned off"));
        }
        // libgit2 asks again after a refused token; asking forever would never end.
        match &remote.session {
            Some(session) if !offered.replace(true) => {
                Cred::userpass_plaintext(session.username.as_str(), session.token.expose_secret())
            }
            _ => Err(git2::Error::new(
                ErrorCode::Auth,
                ErrorClass::Http,
                "Door43 refused the account's token",
            )),
        }
    });
    callbacks.sideband_progress(|_| http.internet.is_on());
    callbacks
}

/// What a libgit2 error means. A call that stopped because the switch went off is
/// [`Door43Error::Offline`], whatever libgit2 made of the stop.
fn failed(http: &Http, route: &str, error: git2::Error) -> Door43Error {
    let route = route.to_owned();
    if !http.internet.is_on() {
        return Door43Error::Offline { route };
    }
    let message = error.message().to_owned();
    match error.code() {
        ErrorCode::Auth => Door43Error::AuthFailed {
            route,
            status: AuthStatus::Unauthorized,
            message,
        },
        _ if message.ends_with("status code: 403") => Door43Error::AuthFailed {
            route,
            status: AuthStatus::Forbidden,
            message,
        },
        _ if message.ends_with("status code: 404") || error.code() == ErrorCode::NotFound => {
            Door43Error::NotFound { route, message }
        }
        _ => Door43Error::Unreachable {
            route,
            cause: match error.class() {
                ErrorClass::Net | ErrorClass::Ssl | ErrorClass::Os => UnreachableCause::Connect,
                _ => UnreachableCause::Other(message),
            },
        },
    }
}
