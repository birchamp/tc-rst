//! What a refused ref of a git push means.
//!
//! Door43 never answers a push without write access with an HTTP 403. It accepts the request
//! (HTTP 200), unpacks the pack, then its pre-receive hook refuses every ref: the hook's text,
//! `User permission denied for writing.`, arrives on side band 2, and the status report says
//! `ng <ref> pre-receive hook declined` (docs/evidence/door43-live-2026-10-17.md, step 6).
//! libgit2's push returns `Ok` for that; it hands the ref's status to the
//! `push_update_reference` callback and the side-band text to `sideband_progress`.

use crate::error::Door43Error;
use crate::names::RepoRef;

/// Door43's pre-receive hook says this to an account that may not write to the repository.
const NO_WRITE_ACCESS: &str = "User permission denied for writing";

/// The error for a ref Door43 refused. `status` is the ref's status from libgit2's
/// `push_update_reference` callback, such as `pre-receive hook declined`; `remote_text` is all
/// that the push's `sideband_progress` callback received.
pub fn refused_push(repo: &RepoRef, status: &str, remote_text: &str) -> Door43Error {
    let route = format!("POST /{repo}.git/git-receive-pack");
    let said: Vec<&str> = remote_text
        .lines()
        .map(|line| line.trim_start_matches("error:").trim())
        .filter(|line| !line.is_empty())
        .collect();
    if said.iter().any(|line| line.contains(NO_WRITE_ACCESS)) {
        return Door43Error::NoWriteAccess {
            route,
            server_message: said.join("\n"),
        };
    }
    let server_message = if said.is_empty() {
        status.to_owned()
    } else {
        format!("{} ({status})", said.join("\n"))
    };
    Door43Error::RefRejected {
        route,
        server_message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> RepoRef {
        "birch/test".parse().unwrap()
    }

    #[test]
    fn door43s_permission_text_is_no_write_access() {
        let error = refused_push(
            &repo(),
            "pre-receive hook declined",
            "error:\nerror: User permission denied for writing.\nerror:\n",
        );
        assert!(
            matches!(
                &error,
                Door43Error::NoWriteAccess { route, server_message }
                    if route == "POST /birch/test.git/git-receive-pack"
                        && server_message == "User permission denied for writing."
            ),
            "{error:?}"
        );
    }

    #[test]
    fn another_refusal_keeps_door43s_text_and_the_status() {
        let error = refused_push(
            &repo(),
            "pre-receive hook declined",
            "error: Not allowed to push to protected branch main\n",
        );
        assert!(
            matches!(
                &error,
                Door43Error::RefRejected { server_message, .. }
                    if server_message
                        == "Not allowed to push to protected branch main (pre-receive hook declined)"
            ),
            "{error:?}"
        );
        let error = refused_push(&repo(), "non-fast-forward", "");
        assert!(
            matches!(&error, Door43Error::RefRejected { server_message, .. }
                if server_message == "non-fast-forward"),
            "{error:?}"
        );
    }
}
