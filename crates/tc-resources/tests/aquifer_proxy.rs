//! Which proxy the GitHub client uses, and when it asks for one. Nothing here reaches a
//! network: the proxy is a local one that refuses every request, and the servers it is
//! asked for are `.test` names that no request ever resolves.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex, PoisonError};

use tc_door43::{InternetSwitch, ProxyAnswer, ProxyFinder, UnreachableCause};
use tc_door43_fake::FakeProxy;
use tc_resources::{AquiferClient, AquiferError, AquiferLimits, AquiferRepo};
use url::Url;

struct Always {
    proxy: Url,
    asked: Mutex<Vec<String>>,
}

impl ProxyFinder for Always {
    fn proxy_for(&self, origin: &Url) -> Option<ProxyAnswer> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(origin.as_str().to_owned());
        Some(ProxyAnswer::Through(self.proxy.clone()))
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn requests_go_through_the_proxy_asked_for_once_per_origin_and_only_with_the_internet_on() {
    let proxy = FakeProxy::start().await.unwrap();
    let finder = Arc::new(Always {
        proxy: proxy.url().parse().unwrap(),
        asked: Mutex::default(),
    });
    let switch = InternetSwitch::new();
    let client = AquiferClient::with_proxy_finder(
        "https://web.test".parse().unwrap(),
        "https://api.test".parse().unwrap(),
        switch.clone(),
        AquiferLimits::default(),
        Some(finder.clone()),
    )
    .unwrap();
    let repo = AquiferRepo::bible_aquifer("UWTranslationNotes").unwrap();

    let offline = client.latest_release(&repo).await.unwrap_err();
    assert!(
        matches!(offline, AquiferError::Offline { .. }),
        "{offline:?}"
    );
    assert!(finder.asked.lock().unwrap().is_empty());
    assert!(proxy.requests().is_empty());

    switch.turn_on();
    let error = client.latest_release(&repo).await.unwrap_err();
    assert!(
        matches!(
            &error,
            AquiferError::Unreachable {
                cause: UnreachableCause::Connect,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(proxy.requests(), ["CONNECT api.test:443 HTTP/1.1"]);
    // Both servers, and the hosts a download may be redirected to.
    assert_eq!(
        *finder.asked.lock().unwrap(),
        [
            "https://web.test/",
            "https://api.test/",
            "https://github.com/",
            "https://objects.githubusercontent.com/",
            "https://release-assets.githubusercontent.com/",
            "https://github-releases.githubusercontent.com/",
        ]
    );
}
