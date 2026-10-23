//! The checks of APP-DESIGN §5.2, each against a real server over a real socket.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::net::{TcpStream, UdpSocket};

use common::{Req, TestServer, stub};
use tc_api::endpoint::API_VERSION_HEADER;
use tc_api::{ApiError, Reply};

fn start() -> TestServer {
    TestServer::start(stub())
}

fn origin(server: &TestServer) -> String {
    format!("http://127.0.0.1:{}", server.port)
}

#[test]
fn request_without_token_is_401() {
    let server = start();
    for path in [
        "/",
        "/ui/app.js",
        "/help/index.html",
        "/nope",
        "/api/v1/events",
    ] {
        let bare = Req::new("GET", path).with("Host", &format!("127.0.0.1:{}", server.port));
        assert_eq!(server.send(&bare).status, 401, "GET {path} without a token");
        let wrong = bare.clone().with("X-Tc-Token", "not-the-token");
        assert_eq!(
            server.send(&wrong).status,
            401,
            "GET {path} with a wrong token"
        );
    }
    let post = server.api_req("app/hello", "{}").without("x-tc-token");
    assert_eq!(server.send(&post).status, 401);
    let wrong = server.api_req("app/hello", "{}").with("X-Tc-Token", "x");
    assert_eq!(server.send(&wrong).status, 401);
    // A token with one byte off, and a prefix of the right one.
    let mut off = server.token.clone().into_bytes();
    off[0] = if off[0] == b'A' { b'B' } else { b'A' };
    let off = String::from_utf8(off).unwrap();
    for bad in [off.as_str(), &server.token[..server.token.len() - 1], ""] {
        let request = server.authed("GET", "/").with("X-Tc-Token", bad);
        assert_eq!(server.send(&request).status, 401, "token {bad:?}");
    }
}

#[test]
fn a_401_names_nothing_and_is_not_cached() {
    let server = start();
    let response =
        server.send(&Req::new("GET", "/").with("Host", &format!("127.0.0.1:{}", server.port)));
    assert_eq!(response.status, 401);
    assert_eq!(response.header("cache-control"), Some("no-store"));
    assert!(!response.text().contains(&server.token));
}

#[test]
fn foreign_host_is_403() {
    let server = start();
    for host in [
        Some("evil.example:1".to_owned()),
        Some(format!("evil.example:{}", server.port)),
        Some(format!("localhost:{}", server.port)),
        Some(format!("127.0.0.1:{}", server.port + 1)),
        Some("127.0.0.1".to_owned()),
        None,
    ] {
        let mut request = server.authed("GET", "/").without("host");
        if let Some(host) = &host {
            request = request.with("Host", host);
        }
        // The token is good: the Host is checked first, so the answer is never 401.
        assert_eq!(server.send(&request).status, 403, "Host {host:?}");
    }
    // Two Host lines are not the server's name either.
    let twice = server
        .authed("GET", "/")
        .add("Host", &format!("127.0.0.1:{}", server.port));
    assert_eq!(server.send(&twice).status, 403);
    let no_token = Req::new("GET", "/").with("Host", "evil.example");
    assert_eq!(server.send(&no_token).status, 403, "403 before 401");
}

#[test]
fn foreign_origin_post_is_403() {
    let server = start();
    let good = server.api_req("app/hello", "{}");
    assert_eq!(server.send(&good).status, 200);
    for bad in [
        "http://evil.example",
        "null",
        "http://127.0.0.1:1",
        "https://127.0.0.1",
    ] {
        let request = good.clone().with("Origin", bad);
        assert_eq!(server.send(&request).status, 403, "Origin {bad}");
    }
    let none = good.clone().without("origin");
    assert_eq!(server.send(&none).status, 403, "no Origin");
    let twice = good.clone().add("Origin", &origin(&server));
    assert_eq!(server.send(&twice).status, 403, "two Origin lines");
    // A GET needs no Origin; HEAD is not GET and needs it.
    assert_eq!(server.get("/").status, 200);
    let head = server.authed("HEAD", "/");
    assert_eq!(server.send(&head).status, 403);
    let head = head.with("Origin", &origin(&server));
    assert_eq!(server.send(&head).status, 200);
}

#[test]
fn wrong_api_version_is_409() {
    let server = start();
    let good = server.api_req("app/hello", "{}");
    for request in [
        good.clone().without(API_VERSION_HEADER),
        good.clone().with(API_VERSION_HEADER, "2"),
        good.clone().with(API_VERSION_HEADER, "01"),
        good.clone().with(API_VERSION_HEADER, "1, 1"),
    ] {
        let response = server.send(&request);
        assert_eq!(response.status, 409);
        let reply: Reply<()> = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(reply, Reply::Err(ApiError::Version { server: 1 }));
    }
    let events = server.authed("GET", "/api/v1/events");
    assert_eq!(server.send(&events).status, 409);
    // The version header is for the API only.
    assert_eq!(server.get("/").status, 200);
}

#[test]
fn no_cors_headers_ever() {
    let server = start();
    let foreign = "http://evil.example";
    let preflight = server
        .authed("OPTIONS", "/api/v1/app/hello")
        .with("Origin", foreign)
        .with("Access-Control-Request-Method", "POST")
        .with("Access-Control-Request-Headers", "x-tc-token");
    let unauth = Req::new("OPTIONS", "/api/v1/app/hello")
        .with("Host", &format!("127.0.0.1:{}", server.port))
        .with("Origin", foreign)
        .with("Access-Control-Request-Method", "POST");
    let requests = [
        preflight.clone(),
        preflight.with("Origin", &origin(&server)),
        unauth,
        server.authed("GET", "/").with("Origin", foreign),
        server.authed("GET", "/ui/app.js"),
        server.authed("GET", "/nope"),
        server.api_req("app/hello", "{}"),
        server.api_req("app/hello", "{}").with("Origin", foreign),
        server
            .api_req("app/hello", "{}")
            .without(API_VERSION_HEADER),
    ];
    for request in requests {
        let response = server.send(&request);
        assert!(
            response
                .headers
                .iter()
                .all(|(name, _)| !name.to_ascii_lowercase().starts_with("access-control-")),
            "{:?} answered {:?}",
            String::from_utf8_lossy(&request.head_bytes()),
            response.headers
        );
    }
}

#[test]
fn binds_loopback_only() {
    let server = start();
    assert!(!server.token.is_empty());
    let local = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
    assert!(local.peer_addr().unwrap().ip().is_loopback());
    // The machine's own non-loopback address, found without sending a packet: a UDP
    // `connect` only picks the route.
    let own = UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| socket.connect("192.0.2.1:9").map(|()| socket))
        .and_then(|socket| socket.local_addr());
    match own {
        Ok(address) if !address.ip().is_loopback() && !address.ip().is_unspecified() => {
            let refused = TcpStream::connect((address.ip(), server.port));
            assert!(
                refused.is_err(),
                "the server answered on {address}: it is not loopback only"
            );
        }
        other => eprintln!("no non-loopback address to try ({other:?}); that half is skipped"),
    }
}

#[test]
fn address_line_names_loopback_and_a_43_character_token() {
    let server = start();
    let token = &server.token;
    assert_eq!(token.len(), 43);
    assert!(
        token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    );
    let other = start();
    assert_ne!(server.token, other.token, "a token is new at every launch");
}

#[cfg(debug_assertions)]
#[test]
fn dev_cookie_only_in_debug_builds() {
    let server = start();
    let host = format!("127.0.0.1:{}", server.port);
    let token = server.token.clone();
    let redirect =
        server.send(&Req::new("GET", &format!("/?dev-token={token}")).with("Host", &host));
    assert_eq!(redirect.status, 303);
    assert_eq!(
        redirect.header("location"),
        Some(format!("/#t={token}").as_str())
    );
    let cookie = redirect.header("set-cookie").unwrap();
    assert_eq!(
        cookie,
        format!("tc4_token={token}; HttpOnly; SameSite=Strict; Path=/")
    );
    // The cookie alone opens a GET, and a wrong one opens nothing.
    let with_cookie = Req::new("GET", "/ui/app.js")
        .with("Host", &host)
        .with("Cookie", &format!("other=1; tc4_token={token}"));
    assert_eq!(server.send(&with_cookie).status, 200);
    let wrong = Req::new("GET", "/ui/app.js")
        .with("Host", &host)
        .with("Cookie", "tc4_token=wrong");
    assert_eq!(server.send(&wrong).status, 401);
    let bad_param = server.send(&Req::new("GET", "/?dev-token=wrong").with("Host", &host));
    assert_eq!(bad_param.status, 401);
    // Only `/` trades the token, and only for a GET.
    let elsewhere =
        server.send(&Req::new("GET", &format!("/ui/app.js?dev-token={token}")).with("Host", &host));
    assert_eq!(elsewhere.status, 401);
    // A cookie does not skip the Origin rule on a write.
    let post = server
        .api_req("app/hello", "{}")
        .without("x-tc-token")
        .with("Cookie", &format!("tc4_token={token}"))
        .without("origin");
    assert_eq!(server.send(&post).status, 403);
}

#[cfg(not(debug_assertions))]
#[test]
fn dev_cookie_only_in_debug_builds() {
    let server = start();
    let host = format!("127.0.0.1:{}", server.port);
    let token = server.token.clone();
    let trade = server.send(&Req::new("GET", &format!("/?dev-token={token}")).with("Host", &host));
    assert_eq!(trade.status, 401);
    assert_eq!(trade.header("set-cookie"), None);
    let cookie = Req::new("GET", "/ui/app.js")
        .with("Host", &host)
        .with("Cookie", &format!("tc4_token={token}"));
    assert_eq!(server.send(&cookie).status, 401);
}

#[test]
fn help_never_redirects() {
    let server = start();
    let page = server.get("/help/index.html");
    assert_eq!(page.status, 200);
    assert_eq!(
        page.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    for path in [
        "/help/",
        "/help/sub",
        "/help/sub/",
        "/help/missing.html",
        "/help",
    ] {
        let response = server.get(path);
        assert_eq!(response.status, 404, "GET {path}");
        assert!(response.header("location").is_none());
    }
    assert_eq!(server.get("/help/sub/page.html").status, 200);
    for path in ["/", "/ui/app.js", "/prepare.html", "/ui/", "/ui"] {
        let status = server.get(path).status;
        assert!(
            !(300..400).contains(&status),
            "GET {path} answered {status}"
        );
    }
}

#[test]
fn index_html_has_the_security_headers() {
    let server = start();
    let index = server.get("/");
    assert_eq!(index.status, 200);
    assert_eq!(
        index.header("content-security-policy"),
        Some(tc_server::CSP)
    );
    assert_eq!(
        tc_server::CSP,
        "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' \
         'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; frame-src 'self'; \
         object-src 'none'; base-uri 'none'"
    );
    assert_eq!(index.header("referrer-policy"), Some("no-referrer"));
    assert_eq!(index.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(index.header("cache-control"), Some("no-store"));
    assert_eq!(
        index.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    // Every other response carries the two safety headers too, errors included.
    for path in ["/ui/app.js", "/nope", "/help/index.html"] {
        let response = server.get(path);
        assert_eq!(
            response.header("x-content-type-options"),
            Some("nosniff"),
            "{path}"
        );
        assert_eq!(
            response.header("referrer-policy"),
            Some("no-referrer"),
            "{path}"
        );
    }
    let refused =
        server.send(&Req::new("GET", "/").with("Host", &format!("127.0.0.1:{}", server.port)));
    assert_eq!(refused.header("x-content-type-options"), Some("nosniff"));
}

#[test]
fn a_repeated_token_header_with_the_same_value_is_accepted() {
    let server = start();
    let token = server.token.clone();
    let twice = server.authed("GET", "/").add("X-Tc-Token", &token);
    assert_eq!(server.send(&twice).status, 200);
    let joined = server
        .authed("GET", "/")
        .with("X-Tc-Token", &format!("{token}, {token}"));
    assert_eq!(server.send(&joined).status, 200);
}

#[test]
fn a_wrong_value_among_repeated_token_headers_is_401() {
    let server = start();
    let token = server.token.clone();
    for request in [
        server.authed("GET", "/").add("X-Tc-Token", "wrong"),
        server
            .authed("GET", "/")
            .with("X-Tc-Token", &format!("{token}, wrong")),
        server
            .authed("GET", "/")
            .with("X-Tc-Token", &format!("wrong,{token}")),
        server
            .authed("GET", "/")
            .with("X-Tc-Token", &format!("{token},")),
    ] {
        assert_eq!(server.send(&request).status, 401);
    }
}

#[test]
fn the_token_is_never_in_what_the_server_prints() {
    let mut server = start();
    let token = server.token.clone();
    server.get("/");
    server.send(&server.authed("GET", "/").with("X-Tc-Token", "wrong"));
    server.quit_message();
    server.join();
    assert!(!server.stdout.text().contains(&token));
}
