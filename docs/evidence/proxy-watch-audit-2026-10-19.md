# proxy-watch for Windows' proxy lookup: the choice and its audit (2026-10-19)

Hazard 6 of `docs/evidence/windows-build-2026-10-17.md`: on a network that hands out its proxy
by PAC or WPAD, the rebuild cannot reach Door43, because `system_proxy_finder()` returns
`None` and reqwest reads only the manual proxy setting. The lookup has to ask WinHTTP, and the
workspace forbids `unsafe` (AGENTS.md). This records which crate does it, and an audit of that
crate, done on the owner's Windows 10 laptop.

How it was done:
- **The survey.** 79 agents searched in four ways: the crate registries, code search for the
  WinHTTP calls, what established Rust projects use, and PAC engines that do not use WinHTTP.
  Two skeptics then vetted each candidate, one reading its source and one building a small
  program against it on this laptop. A judge decided.
- **The audit.** 11 agents took four lenses: the soundness of the Windows FFI, behaviour against
  Microsoft's documentation, the supply chain, and fit with tc-rst. Two skeptics tried to refute
  every blocker and major finding. A judge decided.
- **The draft.** The code on branch `windows-proxy-draft` was written to the conditions below.
  It was **not compiled** on this laptop: the Linux session builds and tests it in its Windows
  VM.

## The pick

- **Crate:** `proxy-watch = { version = "=0.3.1", default-features = false }`. tc-door43
  depends on it only on Windows, with `features = ["pac-windows-native"]`.
- **Source:** crates.io, from the repository https://github.com/gnoays/proxy-watch at tag
  v0.3.1, commit `e1009b0` (`e1009b0f`).
- **Tarball sha256:** `aa5464483809f28430fadd3e7e09cf724e2c4b1e771895fb5066b5a2f523be44`.
- **Licence:** MIT OR Apache-2.0. tc-rst takes the MIT arm, which is compatible with
  GPL-2.0-or-later. Its MIT notice must ship with the app.
- **Reproducible build:** the crates.io tarball rebuilds bit for bit from tag v0.3.1.
- **Dependencies:** it adds no duplicate versions. Its `windows` 0.62.2 and `url` 2.5.8 are
  already in tc-rst's Cargo.lock.
- **What it calls:** `WinHttpGetIEProxyConfigForCurrentUser`, then
  `WinHttpGetProxyForUrlEx` with `WINHTTP_AUTOPROXY_AUTO_DETECT` and/or
  `WINHTTP_AUTOPROXY_CONFIG_URL`, through a safe API, with a time limit on each call.

## The conditions, as sent to the Linux session

These are word for word as sent on 2026-10-19. The audit's own fuller wording is C1 to C14 in
its verdict below.

(a) the finder needs three answers (Proxy / Direct / NoAnswer). Today a PAC "DIRECT" becomes None and falls through to the manual proxy that auto-config overrides, as Edge doesn't.
(b) every Err from read() or resolve_config means NoAnswer, never Direct. A PAC that can't be fetched or run then falls back to SystemProxy's manual setting.
(c) the *_PROXY variables come first: return NoAnswer when HTTPS_PROXY or ALL_PROXY is set.
(d) an outer tokio::time::timeout around the spawn_blocking join (origins × 15 s). On expiry, store no answers and keep for_turn_on, so the lookup isn't restarted for every request.
(e) a new WinHttpPacResolver per lookup, and never call reset_auto_proxy().
(f) the reqwest "socks" feature, which adds no crate in 0.13.5.
(g) WinHTTP refuses a file: PAC URL (12006). Serve the test PAC over loopback http, with a unique path per run, because WinHTTP caches results machine-wide by URL.
(h) never run proxy-watch's own --include-ignored tests on a real machine: they rewrite the user's Internet Settings.
(i) ship its MIT notice.
(j) review its diff of src/pac/winhttp.rs, src/sys/win and build.rs before any version bump.

The audit also asked for a Windows CI job (its C9). The owner declined a Windows CI job on
2026-10-18. The Linux session now runs the Windows gates in a Windows 11 VM.

## The audit's verdict: adopt with conditions

No blocker survived the audits. Reasons:

- **Memory safety.** The ffi lens reviewed every compiled `unsafe` block (src/pac/winhttp.rs:287-941, src/sys/win/mod.rs:514-611, src/sys/win/ffi.rs:85-380) and stress-tested them with an instrumented copy. Over 70 cancelled lookups it saw 0 double callbacks, 0 contexts left outstanding, and no handle or memory growth per call.
- **Routing.** The crate never routed to a host that the settings or the script did not name.
- **Side effects.** It writes nothing, starts no processes and reads no environment variables (supply lens).
- **Provenance.** The tarball rebuilds bit for bit from tag v0.3.1 at e1009b0, and the licence works through the MIT arm.

All three majors are in how tc-rst uses the answer, so tc-rst can fix them in its own safe code. Neither alternative is better: the `winhttp` crate or microsoft/os-proxy-resolver would mean redoing this FFI with less testing, and writing our own needs an unsafe exception.

docs/evidence/windows-build-2026-10-17.md:197-201 leaves the owner to choose between "a crate that opts out of forbid(unsafe_code)" and "a vetted dependency". This verdict is the vetting for the second option. It is not the owner's decision.

### Conditions

#### Cargo.toml

- **C1. Pin the dependency.**
  - Root `[workspace.dependencies]`: `proxy-watch = { version = "=0.3.1", default-features = false }`.
  - crates/tc-door43/Cargo.toml: `[target.'cfg(windows)'.dependencies] proxy-watch = { workspace = true, features = ["pac-windows-native"] }`.
  - Commit Cargo.lock with checksum `aa5464...be44`. Enable no other proxy-watch features.
- **C2. Enable reqwest's `socks` feature** in crates/tc-door43/Cargo.toml:16, or in the workspace reqwest entry.
  - In reqwest 0.13.5 the feature is empty (`socks = []`, Cargo.toml:178) and uses hyper-util's SocksV4 (connect.rs:1883). A skeptic saw a real SOCKS4 handshake with it and no new lock entries.
  - Otherwise: the finder must skip Socks steps, take the next step, and return NoAnswer when no step is left.

#### Finder code (crates/tc-door43/src/http.rs and the new cfg(windows) finder)

- **C3. Give the finder three answers.**
  - Change `ProxyFinder::proxy_for` (http.rs:38-44) to return something like `enum Route { Proxy(Url), Direct, NoAnswer }`.
  - Store Direct in `Found` (http.rs:204-208, 226). `choose()` (http.rs:150-157) must return None for a Direct origin without asking SystemProxy. Only NoAnswer may fall back to SystemProxy.
  - Add a test next to http.rs:454-462: a Direct answer is not overridden by the system's manual proxy.
- **C4. Calls.** Inside the spawn_blocking at http.rs:223, for each origin:
  - Call `proxy_watch::read()`, then a new `WinHttpPacResolver::with_timeout(Duration::from_secs(10))?.with_wpad(true)`, then `resolve_config(&cfg, origin)`.
  - Never call `resolve()` (winhttp.rs:316-320, 410-427), `reset_auto_proxy()` (winhttp.rs:258-298) or `ProxyWatcher`. Nobody reviewed notify.rs for behaviour.
  - Never keep a resolver in shared state (winhttp.rs:750-779).
  - Pass only the https origins from `HttpSpec.origins` (http.rs:63, 105).
- **C5. Map Ok(steps).** Walk the steps in order:
  - Direct gives `Route::Direct`.
  - Http, Https or Socks4 gives `Proxy(step.to_url())`.
  - Skip a non-Direct step whose `to_url()` is None (resolve.rs:64-70).
  - An empty list gives NoAnswer.
- **C6. Map every Err to NoAnswer.**
  - This covers every Err from `read()` or `resolve_config`: Io with any raw code (12167, 12177, 12006, 12015 and others), PacTimeout, PacEvaluation, PacInvalidResult, PacNotSupported and InvalidProxyUrl.
  - SystemProxy then supplies the manual proxy, or DIRECT when there is none. That is the Chromium/Microsoft fallback the crate itself cites (src/sys/win/mod.rs:393-399), and it is how M1 is fixed.
  - Do not refuse the request, and do not map an Err to `Route::Direct`.
  - Log only the error kind and `raw_os_error`. Never log the PAC URL or a proxy URL, because they can carry credentials (resolve.rs:72, winhttp.rs:72-74).
- **C7. Keep environment variables first.**
  - If `HTTPS_PROXY`, `https_proxy`, `ALL_PROXY` or `all_proxy` is set, return NoAnswer without asking proxy-watch. The variables then still win, as http.rs:165-166 promises.
  - Alternative: `ProxyEnv::from_env()` plus `with_env(&env, EnvPrecedence::BeforeSystem)` (examples/reqwest_client.rs:73-75). No lens tested that path.
- **C8. Add an outer timeout.**
  - Wrap the spawn_blocking join (http.rs:223-235) in `tokio::time::timeout` of (number of origins) × 15 s.
  - When it expires, store NoAnswer for every origin and set `for_turn_on`, so a stuck call is not started again for each request.
  - Reason: the 10 s budget bounds only the wait (winhttp.rs:429-430). It does not cover WinHttpOpen, WinHttpCreateProxyResolver or WinHttpCloseHandle (winhttp.rs:783-827, 457).

#### CI and tests

- **C9. Add a windows-latest CI job** that runs `cargo clippy -p tc-door43 --all-targets -- -D warnings` and `cargo test -p tc-door43`. Today ci.yml:11-36 runs on ubuntu only, so the Windows code is never built in CI.
- **C10. Add a Windows-only finder test against a loopback HTTP PAC server.**
  - Do not use a file:/// PAC URL. The behavior lens measured that WinHTTP refuses it with 12006, so the plan at docs/evidence/windows-build-2026-10-17.md:201-204 will not work.
  - Build the config with `ProxyConfig::from_source(ProxyConfigSource::Registry, ProxyMode::pac(url))`, so no settings change.
  - Use a unique PAC URL on every run, because WinHTTP caches PAC results machine-wide (winhttp.rs:322-331).
  - Cases:
    - PROXY gives Proxy.
    - DIRECT gives Direct, not the manual proxy.
    - A bare `SOCKS` entry reaches a fake listener with a SOCKS4 handshake.
    - A 404 or a throwing script gives NoAnswer.
    - `HTTPS h:443` gives a working https:// proxy. This is untested so far.
- **C11. Never run proxy-watch's own tests with `--include-ignored` on a developer machine.** They rewrite HKCU Internet Settings and flush the autoproxy cache (src/sys/win/mod.rs:1060-1100, winhttp.rs:1039-1041).

#### Records

- **C12. Record the decision in hazard 6** (docs/evidence/windows-build-2026-10-17.md:183-205), and get the owner's go-ahead there. Include:
  - the decision and the pinned checksum;
  - the two undocumented WinHTTP behaviours the crate's unsafe code relies on (winhttp.rs:411-416 and 443-457).
- **C13. Before release, ship proxy-watch's MIT notice** (LICENSE-MIT:3) and name the MIT arm. tc-rst has no notices step yet (docs/APP-DESIGN.md:3114).
- **C14. Before any version bump:**
  - review the diff of src/pac/winhttp.rs, src/sys/win/* and build.rs;
  - check that the new version's crates.io `trustpub_data` sha is a commit on main with green CI. Trusted publishing is not enforced for this crate (`trustpub_only=false`).

### Standing findings

**Blocker:** none.

#### Major (both skeptics confirmed each one)

- **M1. A PAC script that cannot be fetched or run gives Err, never the static proxy or DIRECT.**
  - Where: src/sys/win/mod.rs:304-307 (the Pac branch drops ProxyServer); src/pac/winhttp.rs:511-512, 550-556, 561-564, 152-154.
  - The test comment at src/sys/win/mod.rs:970-976 says this case is handled, but the test (982-996) only checks a tuple.
  - Measured: errors 12167 and 12177. AutoDetectThenUrl never returns 12180, so `beneath` cannot be reached when an AutoConfigURL is set.
  - Fixed by C6. What remains:
    - hyper-util reads only the LAN HKCU values, not a RAS/VPN connection (hyper-util matcher.rs:696-718).
    - A script that throws goes to the manual proxy, where Edge goes DIRECT.
- **M2. A PAC answer of DIRECT becomes None, and None makes tc-door43 fall back to the manual proxy that auto-config overrides.**
  - Where: resolve.rs:86-87; tc-door43 http.rs:38-44, 150-157, 226; hyper-util matcher.rs:238-249, 695-724.
  - Fixed by C3.
  - Both skeptics refuted the finding's second half: falling back to the manual proxy on errors is Edge's behaviour. Its suggested "refuse the request" fix is rejected.
- **M3. A SOCKS step cannot be used, because tc-door43 builds reqwest without `socks`.**
  - Where: crates/tc-door43/Cargo.toml:16; winhttp.rs:931-933; parse.rs:166-167, 213; reqwest connect.rs:786-792; hyper-rustls connector.rs:95-97 ("unsupported scheme socks4").
  - The skeptics' probes narrowed it:
    - Only a bare `SOCKS` entry or an IE `socks=` setting triggers it. WinHTTP itself drops the `SOCKS5` and `SOCKS4` keywords.
    - WinHTTP only ever produces Socks4.
    - It fails closed.
  - One skeptic rated it minor. I keep it major: it is a hard failure of every Door43 request on such a network. Fixed by C2.

#### Minor

##### FFI and timeouts

1. **Synchronous return.** The branch that takes back the callback's Arc when WinHTTP returns synchronously relies on an unstated rule, and a synchronous ERROR_SUCCESS is treated as success (winhttp.rs:410-427, 418, 742).
   - On timeout, the arguments are freed after the handle is closed, assuming WinHTTP copied them (443-457).
   - The resolve_config path with http(s) URLs (506, 512-513) never reaches the synchronous branch.
2. **A timed-out lookup keeps running in the service, and a retry right after it fails with 12167** (winhttp.rs:429-463, 333-336). tc-door43 asks only once per internet-switch turn-on (http.rs:213-239), so it never loops.
3. **Session's Send/Sync is inferred, not documented** (winhttp.rs:750-779). C4 avoids relying on it.
4. **resolve()'s doc for a hostless URL is wrong** (winhttp.rs:316-320 vs 410-426).
5. **Only the wait is bounded by the timeout** (winhttp.rs:226, 363, 401-408, 783-827). The measured overrun was at most 80 ms. Covered by C8.

##### Behaviour

6. **Script errors arrive as Io 12177, not the documented PacEvaluation** (winhttp.rs:633-648; doc at 340-343). The OS messages are also unhelpful (ffi.rs:51-53).
7. **An AutoConfigURL that url::Url cannot parse makes read() fail as a whole** (src/sys/win/mod.rs:304-305, 195, 79). A file:/// PAC URL gives 12006. Covered by C6.
8. **A WPAD timeout gives PacTimeout, not the static proxy beneath** (winhttp.rs:556, 457-463). Covered by C6.
9. **WinHTTP mangles some PAC tokens:**
   - `SOCKS5` and `SOCKS4` entries come back with mangled hosts and are dropped.
   - Lowercase `proxy`, `HTTP` and `BOGUS` entries, and a non-string result, give PacInvalidResult.
   - Where: winhttp.rs:928-942, 874-901.
10. **A PAC that defines FindProxyForURLEx is answered by that function**, unlike Edge (winhttp.rs:401-408).
11. **The bypass list uses WinINet's grammar.** `.x` suffix entries, CIDR ranges and scheme:// entries are refused, and only 127.0.0.1 counts as loopback; refused entries go through the proxy, the safe way. Where: bypass.rs:291-298, 309-316, 328-338, 914-917. After C3 this answer is authoritative.
12. **ProxyServer strings that mix a bare host:port with per-scheme tokens** are read in a different order from Chromium (parse.rs:195-198; mode.rs:246-258).
13. **Only the first step is used, so a PAC's own failover such as `PROXY p; DIRECT` is lost** (winhttp.rs:300-310). Accepted: tc-door43 keeps one proxy per origin.
14. **PAC fetches never send domain credentials** (winhttp.rs:382-389). The expected 12015 was not measured. Covered by C6.
15. **Proxy settings that only Edge's own policy keys set are not seen** (src/sys/win/mod.rs:67-91).
16. **Every lookup depends on WinHttpAutoProxySvc and its machine-wide cache** (winhttp.rs:322-331). The crate's user agent at 53-54 is never what gets sent.

##### Supply

17. **One pseudonymous maintainer**, a 6-week-old crate, 1438 downloads, no reverse dependencies (crates.io and GitHub APIs).
18. **Trusted publishing is not enforced** (`trustpub_only=false`; release.yml:49-70). Covered by C14.
19. **Autoproxy state is machine-wide, and WPAD probes go out from the service** (winhttp.rs:240-256, 258-298, 372-390). Never call `reset_auto_proxy`.
20. **WPAD runs on most installs**, because "Automatically detect settings" is the Windows default (src/sys/win/mod.rs:295-303; winhttp.rs:240-245). Windows and Edge behave the same.
21. **Malformed PAC entries pass through as WinHTTP parsed them.** `PROXY user:pw@10.1.1.1:80` comes back as `http://user/` (winhttp.rs:920-958, 948-952).
    - Any WinHTTP client gets the same result.
    - I reject the suggested fix of refusing single-label proxy hosts: `PROXY proxy:8080` is common on intranets.
22. **Upstream CI never tests our exact feature set on Windows** (repo ci.yml:135-144, 175-183, 300-301).
23. **The crate's own ignored tests rewrite HKCU.** Covered by C11.
24. **Licence:** fine through the MIT arm, with one notice obligation (LICENSE-MIT:1-21, Cargo.toml.orig:7). Covered by C13.

##### Fit

25. **The finder's answer beats the `*_PROXY` and `NO_PROXY` variables** (http.rs:157, 165-169; matcher.rs:709-719). Covered by C7.
26. **tc-rst CI never builds the cfg(windows) code** (ci.yml:11-36; AGENTS.md:46-51). Covered by C9.
27. **The shipped Windows app gains 9 windows-family crates.** There are no duplicate versions, but the windows crate is built with different feature sets (Cargo.toml.orig:96-105).
28. **Cargo.lock gains 15 packages that are never built**, which a default cargo-deny run would flag (Cargo.lock 427 -> 442 packages).
29. **tc-rst has no way to ship third-party notices yet** (docs/APP-DESIGN.md:3114).

### Corrections to the reports

- **Contradictory error handling.** The ffi and supply lenses say to treat errors as "proxy unknown" or "show a proxy error". The fit lens says to refuse the request. C6 replaces all three, following the M2 skeptics and src/sys/win/mod.rs:393-399.
- **file:/// PAC test.** The fit lens's CI fix and docs/evidence/windows-build-2026-10-17.md:201-204 both plan a test with a file:/// PAC URL. The behavior lens measured that WinHTTP refuses it with 12006, so C10 uses a loopback HTTP server instead.
- **M3's details.** Its original detail mentions Socks5 and tokio-socks. Both are wrong: WinHTTP yields only Socks4, and reqwest's `socks` feature pulls in no new crates.

### Not covered by the audits

- **Untested environments and configurations:**
  - WinHttpAutoProxySvc stopped or disabled.
  - Any Windows other than Win10 19045: Win11, Server, ARM64 (aarch64-pc-windows-msvc was never built).
  - A real WPAD hit through DHCP option 252 or DNS.
  - A PAC server behind NTLM or Negotiate.
  - IPv6-only networks.
- **Real settings through read().** No lens could test real static, PAC or VPN-connection settings, the HKCU fallback branch, or ALLOW_STATIC with a static proxy present, because each needs a registry change.
- **Proven by reasoning, not by test:**
  - Process exit while a cancel callback is still pending was not tested.
  - The double free on a synchronous return plus a callback cannot be ruled out by testing, only made unlikely.
  - If WinHttpGetIEProxyConfigForCurrentUser or WinHttpGetDefaultProxyConfiguration filled strings before failing, they would leak (src/sys/win/mod.rs:514, 587). Not checked.
- **Code nobody read in full:** ProxyWatcher and notify.rs behaviour (only its unsafe code was reviewed), util.rs, dispatch.rs, policy.rs, auth.rs and hostfn.rs. A grep for `unsafe` found none in them.
- **reqwest behaviour.** Whether an `https://` proxy step works with tc-door43's reqwest and rustls build was not tested.
- **Edge behaviour.** Claims about Chromium came from reading or remembering its source, not from running Edge.
- **The glue code.** None of the tc-rst finder code exists yet. The fit probe only type-checked a sketch. There was no workspace-wide clippy or test run on Windows, and no cargo-vet or cargo-deny run.

## The survey's decision

### Ranking of the candidates that fit

1. **proxy-watch 0.3.1 (the pick).** crates.io, MIT OR Apache-2.0. It follows the browser order and has a 5 s default time limit. It keeps the PAC list up to the first DIRECT, with HTTPS and SOCKS4 entries. It adds no duplicate windows-* versions next to reqwest 0.13.5 and hyper-util 0.1.21. Risks: one maintainer, about 1,400 downloads, first released 2026-09-07, and an API that is still changing.
2. **microsoft/os-proxy-resolver (git rev 796b027c).** This has the best fallback logic: WinHTTP failure goes to the manual proxy plus bypass list, then DIRECT, and it retries with auto-logon on login failure. It ranks second because:
   - it is git-only, so any crate that depends on it cannot be published;
   - it sets no WinHTTP time limit (one vetter measured a 31 s hang);
   - environment variables always win and cannot be switched off;
   - it drops DIRECT, turns a SOCKS result into Direct, and mis-parses the HTTPS token;
   - it adds windows-sys 0.60 (a duplicate) and an extra cdylib build.
3. **winhttp 0.1.3.** The synchronous `get_ie_proxy_config` plus `Session::get_proxy_for_url` path works, but we would write the whole flow ourselves. Problems:
   - the public safe `set_option` is unsound (it segfaulted from code under forbid);
   - the auto-logon option also sets the wrong flag (0x100);
   - there is no LICENSE file;
   - nothing has been released since 2026-02-11;
   - it drops DIRECT, and its Ex/async path is broken.
4. **proxyres 1.5.5.** Published today (8 downloads), and its README says "work in progress". It reads the settings through WinINet, not WinHttpGetIEProxyConfigForCurrentUser. Its order is not the browser's and cannot be changed: a PAC URL means WPAD is skipped, and a manual proxy beats WPAD. A failed PAC gives an error with no fallback. On the plus side it can be time-limited with `wait(Some(t))`, can be cancelled, and retries with auto-logon.
5. **windows-erg 0.2.0.** Every WinHTTP error becomes `Ok(None)`, and after a WPAD miss it never tries the manual proxy. Its `by_scheme` parsing keeps the wrong entry. It pulls in the old, large windows 0.58 crate (about 102 s to build) and compiles a large unrelated API.

**Not a fit:**
- rightkit-http: needs Rust 1.98, brings a second HTTP stack, and its repo is private.
- rama-pac / rama-net: no WPAD, uses its own JS engine, and is very heavy.
- proxyparser: evaluates PAC with Boa (not WinHTTP) and has bugs.
- hyper-util, reqwest, zed-reqwest, proxy_cfg, sysproxy, systemproxy, netru, winproxy, proxyconf, eggress-system-proxy, proxy_config, env_proxy: registry or environment variables only, no PAC.
- nd300: PolyForm-Noncommercial licence.
- system_proxy: every version yanked.
- proxydetect: an empty template.
- autoproxy: binary only, no library.
- wrest, hclient-winhttp, WinRT HttpClient: they replace the HTTP client and expose no lookup.
- win_inet: does not link on MSVC.
- curl: environment variables only.
- codex-http-client: unpublished, Apache-2.0, raw unsafe.
- libparsec: AGPL/BUSL licence and no WPAD.
- boa_engine, rquickjs: JavaScript engines only.

One lead nobody tested properly: windows 0.62's safe `NetworkInformation::GetProxyConfigurationAsync`. Whether it does PAC or WPAD is unknown.

### Dependency line

```toml
[target.'cfg(windows)'.dependencies]
proxy-watch = { version = "=0.3.1", default-features = false, features = ["pac-windows-native"] }
windows-registry = "0.6"   # only for our manual-proxy fallback; already in the tree via hyper-util (0.6.1)
```

- `pac-windows-native` turns on `pac`, which turns on `resolve` (Cargo.toml.orig `[features]`). The defaults (`resolve`, `linux-gnome`, `linux-kde`) are not needed.
- Crate MSRV is 1.88. I built my probe with `cargo +1.97.0` under `[lints.rust] unsafe_code = "forbid"` plus `#![forbid(unsafe_code)]`, and it built cleanly.
- Licence: take the MIT option.

### Call sequence

I compiled and ran this. It runs inside `tokio::task::spawn_blocking`.

```rust
use proxy_watch::pac::WinHttpPacResolver;
use proxy_watch::{parse, ProxyConfig, ProxyConfigSource, ProxyMode, ProxyStep, Url};

fn proxy_for(origin: &Url) -> Result<Option<Url>, proxy_watch::Error> {
    let cfg = proxy_watch::read()?;                         // IE/Edge settings via WinHttpGetIEProxyConfigForCurrentUser
    let r = WinHttpPacResolver::with_timeout(Duration::from_secs(10))?.with_wpad(true);
    let steps = match r.resolve_config(&cfg, origin) {      // WinHttpGetProxyForUrlEx, AUTO_DETECT|CONFIG_URL
        Ok(s) => s,
        Err(_) if matches!(cfg.effective, ProxyMode::Pac { .. } | ProxyMode::WpadAutoDetect) => manual_fallback(origin)?,
        Err(e) => return Err(e),
    };
    Ok(match steps.first() { Some(s) if !s.is_direct() => s.to_url(), _ => None })
}

// manual_fallback: windows_registry::CURRENT_USER.open(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
//   -> if get_u32("ProxyEnable") == 1: parse::windows_manual(&ProxyServer, &ProxyOverride)
//   -> proxy_watch::resolve(&ProxyConfig::from_source(ProxyConfigSource::Registry, mode), url); otherwise [Direct]
```

### How it behaves

**Settings first: the crate does this, not us.**
- `read()` (watch.rs:448) calls `sys/win/mod.rs:69 read_config`, which calls WinHttpGetIEProxyConfigForCurrentUser (`:514`).
- If that call fails, it reads the HKCU Internet Settings values instead (`:190-230`).
- The mode is chosen at `:295-312`: auto-detect on gives WpadAutoDetect; otherwise an AutoConfigURL gives Pac(url); otherwise a ProxyServer plus ProxyOverride gives Manual; otherwise Direct.
- For WpadAutoDetect, `resolve_wpad_with_fallback` (pac/winhttp.rs:523-580) reads the IE config again (`sys/win/mod.rs:336`) to find any AutoConfigURL and manual proxy configured beneath auto-detect.
- `read()` does not read `*_proxy` environment variables; they are separate (`ProxyEnv::from_env`, `ProxyConfig::with_env`).

**Per case:**

| Situation | Result |
|---|---|
| Nothing configured, auto-detect off | `[Direct]`, no WinHTTP call (`resolve.rs:172`) |
| Windows 10 default: auto-detect on, no WPAD server | `[Direct]`. 11 ms in my run, 6-120 ms in the vetters' runs (the service caches the miss) |
| WPAD finds nothing | If an AutoConfigURL is set, WinHTTP tries it in the same call. Otherwise the manual proxy beneath is used with its bypass list, else `[Direct]` (`winhttp.rs:565-578`) |
| PAC URL unreachable or 404 | `Err(Error::Io)` with OS error 12167 (0x2f87). About 30 ms for a 404, 2.0-2.4 s for a closed port (2.03 s in my run). A server that stalls gives `Error::PacTimeout` at the time limit. The manual proxy beneath is not tried (`winhttp.rs:561-564`), which is why we add the fallback |
| PAC script syntax error or throw | `Err(Io 12177)`. Only 12166 becomes `PacEvaluation` (`winhttp.rs:633-648`) |
| PAC returns DIRECT | `[Direct]` |
| `"PROXY a; PROXY b"` | `[Http(a), Http(b)]` |
| `"PROXY a; DIRECT"` | `[Http(a), Direct]`. Anything after the first DIRECT is lost (WinHTTP behaviour, documented in the crate) |
| `SOCKS h:p` | `Socks4`, and `to_url()` gives `socks4://h:p` (`winhttp.rs:931-933`) |
| `SOCKS5 h:p` | WinHTTP drops it: it disappears from a mixed list, and on its own gives `Err(PacInvalidResult)` |
| `HTTPS h:p` | `Https`, and `to_url()` gives `https://h:p` |
| PAC file behind NTLM or Negotiate login | Fails. `fAutoLogonIfChallenged` is FALSE and there is no retry (`winhttp.rs:382-389`) |
| https URLs | The path and query are removed before the script sees the URL. That makes no difference for an origin |

**Blocking and time limits:**
- `read()` takes 8-40 ms.
- Each resolution has a time limit: 5 s by default (`DEFAULT_WINHTTP_PAC_TIMEOUT`, winhttp.rs:51), set with `with_timeout(d)`; zero is an error.
- The limit is applied two ways. WinHttpSetTimeouts sets all four phases to it. WaitForSingleObject waits for it (`:430`), and on expiry the handle is closed, which cancels the call and returns `Error::PacTimeout` (one vetter measured 2025 ms with a 2 s limit).
- A cancelled operation keeps draining in the background, so a retry made straight away can wait behind it.

### What we must wrap

1. Run the lookup in `spawn_blocking`.
2. Always call `.with_wpad(true)`. Without it you get `PacNotSupported`, which must never be read as Direct.
3. Add the manual-proxy fallback when a PAC or WPAD lookup errors. Without it, a PAC failure is an error rather than a proxy. If there is no manual proxy, choosing DIRECT or failing closed is our policy decision.
4. Our raw-registry fallback reads the LAN settings. The crate's own read covers the active connection, such as a VPN.
5. The first step decides the route. `to_url()` includes proxy credentials, so never log it. Trying later steps when the first proxy fails is our job.
6. SOCKS steps need reqwest's `socks` feature.
7. Classify errors by `io::Error::raw_os_error()`: 12167, 12177, 12166, 12015, 12180. Their message text is empty (FormatMessageW failed in my run).
8. Decide whether environment variables should come before the system settings.
9. Create a new `WinHttpPacResolver` for each lookup that misses our cache. A session remembers a WPAD miss for its whole life, so a long-lived one would not notice a network change.
10. Never call `reset_auto_proxy()` routinely: it clears the cache for the whole machine.
11. Pin `=0.3.1` and review the source diff on every upgrade.
12. Deep target paths hit MAX_PATH (LNK1104) on this machine; use a short `CARGO_TARGET_DIR`.

### What I could not verify

- **Real proxy settings:** no system settings were changed, so a real AutoConfigURL in the registry and a manual proxy beneath WPAD were never exercised live. The manual branch of my fallback is also untested, because ProxyEnable is 0 here.
- **Network setups not available here:** a real WPAD hit, a PAC file behind NTLM or Negotiate, VPN connections, a disabled WinHTTP Auto-Proxy Service, and Windows 11.
- **Fit with tc-rst:** C:\src\tc-rst was not opened. The duplicate-version check was done against a fresh reqwest 0.13.5 plus hyper-util 0.1.21 tree only.
- **reqwest:** whether reqwest 0.13 accepts `socks4://` proxies.
- **The crate's internal unsafe:** its callback ownership, and `Session` being Sync by inference (`winhttp.rs:770-776`), were only read and stress-run by the vetters, not audited.
- **The vetting reports:** I checked the source of every proxy-watch claim used above. The other candidates' claims rest on the two vetting reports each.
