//! A fake GitHub for Aquifer release assets: release downloads (redirected to an asset
//! host, as github.com does), `releases/latest` and `commits/{ref}`. It listens on
//! loopback and records every request.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, Response, StatusCode, header};
use axum::routing::get;
use tc_resources::Sha256;
use tokio::sync::oneshot;
use url::Url;

#[derive(Clone)]
pub struct FakeAsset {
    pub name: String,
    pub bytes: Vec<u8>,
    /// Whether the release lists GitHub's digest for the asset.
    pub listed_digest: bool,
}

#[derive(Clone)]
pub struct FakeRelease {
    pub owner: String,
    pub repo: String,
    pub tag: String,
    pub commit: String,
    pub published_at: String,
    pub assets: Vec<FakeAsset>,
}

/// What the next asset download does instead of answering normally.
#[derive(Clone)]
pub enum AssetFault {
    /// Promise the whole asset, send half, and close.
    Truncate,
    /// Serve these bytes, as after a maintainer re-uploaded the asset.
    Replace(Vec<u8>),
}

#[derive(Default)]
struct Inner {
    releases: Vec<FakeRelease>,
    requests: Vec<String>,
    fault: Option<AssetFault>,
}

type Shared = Arc<Mutex<Inner>>;

pub struct FakeGithub {
    url: Url,
    inner: Shared,
    _stop: oneshot::Sender<()>,
}

impl FakeGithub {
    pub async fn start() -> Self {
        let inner: Shared = Arc::default();
        let app = Router::new()
            .route(
                "/{owner}/{repo}/releases/download/{tag}/{asset}",
                get(download),
            )
            .route("/_assets/{owner}/{repo}/{tag}/{asset}", get(asset))
            .route("/repos/{owner}/{repo}/releases/latest", get(latest))
            .route("/repos/{owner}/{repo}/commits/{reference}", get(commit))
            .with_state(inner.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
        });
        Self {
            url: format!("http://{address}/").parse().unwrap(),
            inner,
            _stop: stop,
        }
    }

    pub fn url(&self) -> Url {
        self.url.clone()
    }

    pub fn add_release(&self, release: FakeRelease) {
        self.inner.lock().unwrap().releases.push(release);
    }

    pub fn requests(&self) -> Vec<String> {
        self.inner.lock().unwrap().requests.clone()
    }

    pub fn clear_requests(&self) {
        self.inner.lock().unwrap().requests.clear();
    }

    pub fn fault_next_download(&self, fault: AssetFault) {
        self.inner.lock().unwrap().fault = Some(fault);
    }
}

fn record(shared: &Shared, path: String) {
    shared.lock().unwrap().requests.push(path);
}

fn not_found() -> Response<Body> {
    let mut response = Response::new(Body::from(r#"{"message":"Not Found"}"#));
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}

fn json(value: serde_json::Value) -> Response<Body> {
    let mut response = Response::new(Body::from(value.to_string()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

fn find(shared: &Shared, owner: &str, repo: &str, tag: &str) -> Option<FakeRelease> {
    shared
        .lock()
        .unwrap()
        .releases
        .iter()
        .find(|r| r.owner == owner && r.repo == repo && r.tag == tag)
        .cloned()
}

async fn download(
    State(shared): State<Shared>,
    Path((owner, repo, tag, asset)): Path<(String, String, String, String)>,
) -> Response<Body> {
    record(
        &shared,
        format!("/{owner}/{repo}/releases/download/{tag}/{asset}"),
    );
    if find(&shared, &owner, &repo, &tag).is_none() {
        return not_found();
    }
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::FOUND;
    let location = format!("/_assets/{owner}/{repo}/{tag}/{asset}");
    response
        .headers_mut()
        .insert(header::LOCATION, HeaderValue::from_str(&location).unwrap());
    response
}

async fn asset(
    State(shared): State<Shared>,
    Path((owner, repo, tag, name)): Path<(String, String, String, String)>,
) -> Response<Body> {
    record(&shared, format!("/_assets/{owner}/{repo}/{tag}/{name}"));
    let Some(release) = find(&shared, &owner, &repo, &tag) else {
        return not_found();
    };
    let Some(asset) = release.assets.iter().find(|a| a.name == name) else {
        return not_found();
    };
    let fault = shared.lock().unwrap().fault.take();
    let (body, length) = match fault {
        None => (asset.bytes.clone(), asset.bytes.len()),
        Some(AssetFault::Replace(bytes)) => {
            let len = bytes.len();
            (bytes, len)
        }
        Some(AssetFault::Truncate) => (
            asset.bytes[..asset.bytes.len() / 2].to_vec(),
            asset.bytes.len(),
        ),
    };
    let mut response = Response::new(Body::from(body));
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string()).unwrap(),
    );
    response
}

async fn latest(
    State(shared): State<Shared>,
    Path((owner, repo)): Path<(String, String)>,
) -> Response<Body> {
    record(&shared, format!("/repos/{owner}/{repo}/releases/latest"));
    let newest = shared
        .lock()
        .unwrap()
        .releases
        .iter()
        .rev()
        .find(|r| r.owner == owner && r.repo == repo)
        .cloned();
    let Some(release) = newest else {
        return not_found();
    };
    let assets: Vec<serde_json::Value> = release
        .assets
        .iter()
        .map(|a| {
            let mut v = serde_json::json!({"name": a.name, "size": a.bytes.len()});
            if a.listed_digest {
                v["digest"] = format!("sha256:{}", Sha256::of(&a.bytes)).into();
            }
            v
        })
        .collect();
    json(serde_json::json!({
        "tag_name": release.tag,
        "published_at": release.published_at,
        "draft": false,
        "prerelease": false,
        "assets": assets,
    }))
}

async fn commit(
    State(shared): State<Shared>,
    Path((owner, repo, reference)): Path<(String, String, String)>,
) -> Response<Body> {
    record(
        &shared,
        format!("/repos/{owner}/{repo}/commits/{reference}"),
    );
    match find(&shared, &owner, &repo, &reference) {
        Some(release) => json(serde_json::json!({"sha": release.commit})),
        None => not_found(),
    }
}
