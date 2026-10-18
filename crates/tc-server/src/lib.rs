//! The local HTTP server of the tC4 rebuild (APP-DESIGN §5).
//!
//! It listens on `127.0.0.1` only, on a port the operating system picks, and refuses every
//! request that does not carry the launch's token, the server's own `Host`, and for anything
//! that changes state its own `Origin`. It routes `POST /api/v1/{service}/{name}` for exactly
//! the rows of tc-api's tables to a [`Backend`], streams events, serves the UI's files, takes
//! uploads and hands out finished files. It ends when the shell says so, when stdin closes
//! (the shell died) or when the application asks to quit, and answers every request with 503
//! while it closes.
//!
//! The server knows nothing of projects. Parsing a request, running it and writing the reply
//! are the backend's: `tc-app` in the product, [`stub::Stub`] in this crate's tests.

mod backend;
mod downloads;
mod events;
mod lifecycle;
mod log;
mod routes;
mod security;
mod state;
mod statics;
pub mod stub;
mod token;
mod uploads;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::middleware::from_fn_with_state;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::serve::ListenerExt as _;
use tc_api::upload::UploadFlow;
use tokio::io::{AsyncBufRead, AsyncWrite};
use tokio::sync::watch;

pub use axum::body::Bytes;
pub use backend::{Answer, Backend, EventSource, Next, Then};
pub use downloads::{DownloadError, Downloads};
pub use lifecycle::Exit;
pub use routes::product_paths;
#[cfg(feature = "test-hooks")]
pub use routes::test_paths;
pub use statics::CSP;
pub use token::{AddressLine, LaunchToken};
pub use uploads::{Upload, Uploads};

use state::{AppState, Shared};

/// The sizes and times the server keeps to. [`Limits::default`] holds the product's values;
/// tests pass smaller ones.
#[derive(Debug, Clone)]
pub struct Limits {
    /// The most bytes an API request body may hold: 8 MiB. The largest designed bodies, a
    /// section save of Psalm 119, are well under 1 MiB.
    pub api_body: usize,
    /// How long a quiet event stream waits before it writes a comment line: 15 s.
    pub heartbeat: Duration,
    /// The whole close: the drain of requests being answered and the backend's quit work
    /// share this one budget, 5 s. The cap exists so a hung disk never keeps the process of
    /// a dead shell alive holding the locks; what the store wrote is durable.
    pub close_within: Duration,
    /// How long requests being answered, and backend calls still running, get to finish once
    /// closing began, before the backend's quit work starts: 1 s.
    pub drain_within: Duration,
    /// The most bytes an upload of a flow may carry. The product's value is
    /// [`UploadFlow::cap`]; a test passes a smaller one.
    pub upload_cap: fn(UploadFlow) -> u64,
    /// How long a finished file stays available for download: 10 minutes.
    pub downloads_keep: Duration,
    /// How often expired downloads are looked for: a minute.
    pub downloads_sweep: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            api_body: 8 * 1024 * 1024,
            heartbeat: Duration::from_secs(15),
            close_within: Duration::from_secs(5),
            drain_within: Duration::from_secs(1),
            upload_cap: UploadFlow::cap,
            downloads_keep: Duration::from_secs(10 * 60),
            downloads_sweep: Duration::from_secs(60),
        }
    }
}

pub struct ServerConfig {
    /// The app-data folder: the server keeps its temporary files in it.
    pub app_data: PathBuf,
    /// The folder `--ui` names: `index.html`, `ui/`, `help/`, `prepare.html`.
    pub ui_dir: PathBuf,
    pub limits: Limits,
    /// End the run on Ctrl+C and SIGTERM. A test run leaves it off and keeps its Ctrl+C.
    pub handle_signals: bool,
}

impl ServerConfig {
    pub fn new(app_data: PathBuf, ui_dir: PathBuf) -> Self {
        Self {
            app_data,
            ui_dir,
            limits: Limits::default(),
            handle_signals: false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("cannot listen on 127.0.0.1: {0}")]
    Bind(#[source] io::Error),
    #[error("{} is not a folder", .0.display())]
    UiDir(PathBuf),
    #[error("cannot use the app-data folder {}: {source}", path.display())]
    AppData {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the operating system gave no random bytes")]
    Random,
    #[error("the server stopped: {0}")]
    Serve(#[source] io::Error),
}

/// A server that is listening and has not served a request yet.
pub struct Server {
    listener: std::net::TcpListener,
    shared: Arc<Shared>,
    handle_signals: bool,
    address: AddressLine,
}

/// What the application's flows reach the server's files through.
#[derive(Clone)]
pub struct ServerHandle {
    uploads: Arc<Uploads>,
    downloads: Arc<Downloads>,
}

impl ServerHandle {
    pub fn uploads(&self) -> &Arc<Uploads> {
        &self.uploads
    }

    pub fn downloads(&self) -> &Arc<Downloads> {
        &self.downloads
    }
}

/// Standard input and output of the process, or stand-ins for them.
pub struct Stdio<R, W> {
    /// Lines from the shell: `{"quit":true}`. The end of input means the shell is gone.
    pub input: R,
    /// Where `{"closed":true}` goes.
    pub output: W,
}

impl Server {
    /// Binds `127.0.0.1:0` and picks the launch's token. Nothing is served until
    /// [`Server::run`], so the caller can print [`Server::address`] first.
    pub fn bind(config: ServerConfig) -> Result<Server, ServerError> {
        let ServerConfig {
            app_data,
            ui_dir,
            limits,
            handle_signals,
        } = config;
        if !ui_dir.is_dir() {
            return Err(ServerError::UiDir(ui_dir));
        }
        let uploads_dir = fresh_folder(&app_data, "uploads")?;
        let downloads_dir = fresh_folder(&app_data, "downloads")?;
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(ServerError::Bind)?;
        let port = listener.local_addr().map_err(ServerError::Bind)?.port();
        let token = LaunchToken::new()?;
        let address = AddressLine {
            url: format!("http://127.0.0.1:{port}/"),
            token: token.as_str().to_owned(),
        };
        let downloads_keep = limits.downloads_keep;
        let shared = Arc::new(Shared {
            token,
            host: format!("127.0.0.1:{port}"),
            origin: format!("http://127.0.0.1:{port}"),
            limits,
            ui_dir,
            routes: routes::RouteTable::new(),
            uploads: Arc::new(Uploads::new(uploads_dir)),
            downloads: Arc::new(Downloads::new(downloads_dir, downloads_keep)),
            test_hooks: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            in_flight: AtomicUsize::new(0),
            streams_end: watch::channel(false).0,
        });
        Ok(Server {
            listener,
            shared,
            handle_signals,
            address,
        })
    }

    /// The line `tc4 serve` prints first.
    pub fn address(&self) -> &AddressLine {
        &self.address
    }

    /// The registries of uploads and downloads, which the application's flows use.
    pub fn handle(&self) -> ServerHandle {
        ServerHandle {
            uploads: Arc::clone(&self.shared.uploads),
            downloads: Arc::clone(&self.shared.downloads),
        }
    }

    /// Routes the `test` service too. Without the cargo feature there is no such method, and
    /// without this call the rows answer 404 even in a build that has them.
    #[cfg(feature = "test-hooks")]
    #[must_use]
    pub fn with_test_hooks(self) -> Self {
        self.shared.test_hooks.store(true, Ordering::Relaxed);
        self
    }

    /// Serves until the shell, the application or a signal says to quit, then closes, all
    /// within [`Limits::close_within`]: new requests get 503; requests being answered and
    /// backend calls still running get [`Limits::drain_within`] to finish, so a save accepted
    /// before the quit reaches a backend that is still open and its reply reaches the UI;
    /// then the backend's quit work runs with what is left of the budget; the event streams
    /// end; and unless stdin had closed, `{"closed":true}` goes to the shell. The caller then
    /// ends the process.
    pub async fn run<B, R, W>(
        self,
        backend: Arc<B>,
        stdio: Stdio<R, W>,
    ) -> Result<Exit, ServerError>
    where
        B: Backend,
        R: AsyncBufRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send,
    {
        let Stdio { input, mut output } = stdio;
        let Server {
            listener,
            shared,
            handle_signals,
            address: _,
        } = self;
        listener.set_nonblocking(true).map_err(ServerError::Serve)?;
        let listener = tokio::net::TcpListener::from_std(listener)
            .map_err(ServerError::Serve)?
            // Nagle's algorithm with delayed acknowledgements can hold a small reply on
            // loopback for 40 ms, more than the aligner's whole round-trip budget.
            .tap_io(|stream| {
                let _ = stream.set_nodelay(true);
            });
        let state = AppState {
            shared: Arc::clone(&shared),
            backend: Arc::clone(&backend),
        };
        let mut serving = tokio::spawn(axum::serve(listener, router(state)).into_future());
        let sweeper = tokio::spawn(sweep_downloads(Arc::clone(&shared)));

        let cause = tokio::select! {
            exit = lifecycle::watch_stdin(input) => exit,
            () = backend.quitting() => Exit::AppQuit,
            exit = lifecycle::watch_signals(handle_signals) => exit,
            ended = &mut serving => {
                sweeper.abort();
                return Err(ServerError::Serve(match ended {
                    Ok(Err(error)) => error,
                    Ok(Ok(())) => io::Error::other("the listener closed"),
                    Err(error) => io::Error::other(error),
                }));
            }
        };

        shared.closing.store(true, Ordering::SeqCst);
        let deadline = tokio::time::Instant::now() + shared.limits.close_within;
        let drain_within = shared.limits.drain_within.min(shared.limits.close_within);
        if lifecycle::drain(&shared, drain_within).await {
            // The replies that just finished are handed to their sockets by tasks of
            // their own; let them run before the backend closes.
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        if tokio::time::timeout_at(deadline, backend.quit())
            .await
            .is_err()
        {
            log::warn("the app did not finish closing in time; what it wrote is kept");
        }
        shared.streams_end.send_replace(true);
        if cause != Exit::StdinClosed {
            lifecycle::say_closed(&mut output).await;
        }
        serving.abort();
        sweeper.abort();
        Ok(cause)
    }
}

fn router<B: Backend>(state: AppState<B>) -> Router {
    let shared = Arc::clone(&state.shared);
    Router::new()
        .route("/", get(statics::index))
        .route("/prepare.html", get(statics::prepare))
        .route("/ui/{*path}", get(statics::ui))
        .route("/help/{*path}", get(statics::help))
        .route(tc_api::endpoint::EVENTS_PATH, get(events::stream::<B>))
        .route(
            tc_api::upload::UPLOADS_PATH,
            // The upload streams its own body to a file, against the flow's cap.
            post(uploads::upload).layer(DefaultBodyLimit::disable()),
        )
        .route("/download/{id}/{name}", get(downloads::download))
        .route(
            "/api/v1/{service}/{name}",
            // The handler reads the body itself, against `Limits::api_body`.
            post(routes::api::<B>).layer(DefaultBodyLimit::disable()),
        )
        // The fallback is set before the layers: `Router::layer` skips routes added after it,
        // and an unknown path must still be refused for want of a token, not answered 404.
        .fallback(not_found)
        .layer(from_fn_with_state(Arc::clone(&shared), lifecycle::gate))
        .layer(from_fn_with_state(shared, security::guard))
        .with_state(state)
}

/// Empties `<app_data>/<name>` and creates it: the registries only serve ids of this run, so
/// what an earlier run left there is garbage. A folder that cannot be emptied is logged and
/// kept; one that cannot be created is an error.
fn fresh_folder(app_data: &std::path::Path, name: &str) -> Result<PathBuf, ServerError> {
    let dir = app_data.join(name);
    // `remove_dir_all` does not follow a symbolic link: it removes the link itself.
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => log::warn(format_args!("could not empty {}: {error}", dir.display())),
    }
    std::fs::create_dir_all(&dir).map_err(|source| ServerError::AppData {
        path: dir.clone(),
        source,
    })?;
    Ok(dir)
}

/// Deletes the downloads that have been kept long enough.
async fn sweep_downloads(shared: Arc<Shared>) {
    let mut tick = tokio::time::interval(shared.limits.downloads_sweep);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        for file in shared.downloads.sweep(std::time::Instant::now()) {
            // Already gone is as good as deleted.
            let _ = tokio::fs::remove_file(file).await;
        }
    }
}

async fn not_found() -> impl IntoResponse {
    StatusCode::NOT_FOUND
}
