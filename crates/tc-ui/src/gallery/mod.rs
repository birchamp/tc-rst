//! The design-system gallery: every case of `parity/ds-gallery/gallery-spec.json`, drawn by
//! the Leptos port with the protocol the tC4 baseline was captured with.
//!
//! `?case=<id>` draws one case alone, its frame at the top-left corner of the page. The case
//! mounts when `window.__mount()` is called (or at once with `&auto=1`), so the capture can
//! click the pointer first, exactly as `parity/ds-gallery/src/main.jsx` does. Without `case`
//! the page lists every case, each in an iframe of the first mode.

mod components;
mod props;
mod render;

use leptos::prelude::*;
use serde_json::Value;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async {
        if let Err(e) = run().await {
            web_sys::console::error_1(&e);
        }
    });
}

fn window() -> web_sys::Window {
    leptos::prelude::window()
}

async fn run() -> Result<(), JsValue> {
    let spec = load_spec().await?;
    let search = window().location().search()?;
    let params = web_sys::UrlSearchParams::new_with_str(&search)?;
    match params.get("case") {
        Some(id) => single(&spec, &id, params.get("auto").as_deref() == Some("1")).await,
        None => {
            browse(&spec);
            Ok(())
        }
    }
}

async fn load_spec() -> Result<Value, JsValue> {
    let resp: web_sys::Response = JsFuture::from(window().fetch_with_str("gallery-spec.json"))
        .await?
        .dyn_into()?;
    let text = JsFuture::from(resp.text()?).await?;
    let text = text.as_string().unwrap_or_default();
    serde_json::from_str(&text).map_err(|e| JsValue::from_str(&format!("gallery-spec.json: {e}")))
}

fn root() -> Result<web_sys::HtmlElement, JsValue> {
    document()
        .get_element_by_id("root")
        .and_then(|e| e.dyn_into().ok())
        .ok_or_else(|| JsValue::from_str("no #root"))
}

async fn next_frame() -> Result<(), JsValue> {
    let p = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().request_animation_frame(&resolve);
    });
    JsFuture::from(p).await.map(|_| ())
}

/// Every `@font-face` loaded, as the baseline does before it mounts anything.
async fn load_all_fonts() -> Result<(), JsValue> {
    if document().ready_state() != "complete" {
        let p = js_sys::Promise::new(&mut |resolve, _| {
            let opts = web_sys::AddEventListenerOptions::new();
            opts.set_once(true);
            let _ = window().add_event_listener_with_callback_and_add_event_listener_options(
                "load", &resolve, &opts,
            );
        });
        JsFuture::from(p).await?;
    }
    let fonts = document().fonts();
    let all = js_sys::Array::from(&fonts);
    let loads = js_sys::Array::new();
    // A face that fails to load is reported by the page, not here.
    let ignore = Closure::<dyn FnMut(JsValue)>::new(|_| {});
    for f in all.iter() {
        let face: web_sys::FontFace = f.unchecked_into();
        if let Ok(p) = face.load() {
            loads.push(&p.catch(&ignore));
        }
    }
    JsFuture::from(js_sys::Promise::all(&loads)).await?;
    drop(ignore);
    JsFuture::from(fonts.ready()?).await?;
    Ok(())
}

fn set_global(name: &str, value: &JsValue) {
    let _ = js_sys::Reflect::set(&window(), &JsValue::from_str(name), value);
}

async fn single(spec: &Value, id: &str, auto: bool) -> Result<(), JsValue> {
    let Some(case) = spec["cases"]
        .as_array()
        .and_then(|cs| cs.iter().find(|c| c["id"] == id))
        .cloned()
    else {
        let id = id.to_string();
        leptos::mount::mount_to(root()?, move || view! { <p>"No case " {id}</p> }).forget();
        return Ok(());
    };
    let html = document()
        .document_element()
        .ok_or_else(|| JsValue::from_str("no <html>"))?;
    html.set_attribute("lang", case["document"]["lang"].as_str().unwrap_or("en"))?;
    html.set_attribute("dir", case["document"]["dir"].as_str().unwrap_or("ltr"))?;
    if let Some(body) = document().body() {
        body.style().set_property(
            "background",
            case["frame"]["background"].as_str().unwrap_or(""),
        )?;
    }
    load_all_fonts().await?;

    let mounted = RwSignal::new(false);
    let frame_case = case.clone();
    let handle = leptos::mount::mount_to(root()?, move || {
        render::frame(frame_case.clone(), mounted, false)
    });
    handle.forget();

    let mount = Closure::<dyn Fn() -> js_sys::Promise>::new(move || {
        mounted.set(true);
        wasm_bindgen_futures::future_to_promise(async {
            next_frame().await?;
            next_frame().await?;
            set_global("__galleryMounted", &JsValue::TRUE);
            Ok(JsValue::UNDEFINED)
        })
    });
    let mount = mount.into_js_value();
    set_global("__mount", &mount);
    next_frame().await?;
    set_global("__galleryBooted", &JsValue::TRUE);
    if auto {
        let f: js_sys::Function = mount.unchecked_into();
        JsFuture::from(js_sys::Promise::from(f.call0(&JsValue::NULL)?)).await?;
    }
    Ok(())
}

fn browse(spec: &Value) {
    let cases = spec["cases"].as_array().cloned().unwrap_or_default();
    let count = cases.len();
    let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
    for c in cases {
        let comp = c["component"].as_str().unwrap_or("").to_string();
        match groups.last_mut() {
            Some((g, list)) if *g == comp => list.push(c),
            _ => groups.push((comp, vec![c])),
        }
    }
    let Ok(root) = root() else { return };
    let page = move || {
        let nav = groups
            .iter()
            .map(|(g, _)| view! { <a href=format!("#{g}")>{g.clone()}</a> })
            .collect_view();
        let sections = groups
            .iter()
            .map(|(g, list)| {
                let figures = list
                    .iter()
                    .map(|c| {
                        let id = c["id"].as_str().unwrap_or("").to_string();
                        let w = c["frame"]["width"].as_f64().unwrap_or(0.0);
                        let h = c["frame"]["height"].as_f64().unwrap_or(0.0);
                        let state = c["state"].as_str().unwrap_or("").to_string();
                        view! {
                            <figure style="margin: 0;">
                                <figcaption style="font: 12px ui-monospace, monospace; margin-bottom: 4px;">{state}</figcaption>
                                <div style="outline: 1px dashed rgba(1,66,99,.25);">
                                    {crate::ds::element("iframe")
                                        .attr("title", id.clone())
                                        .attr("src", format!("?case={}&auto=1", String::from(js_sys::encode_uri_component(&id))))
                                        .attr("loading", "lazy")
                                        .attr("style", format!("display: block; border: 0; width: {w}px; height: {h}px;"))}
                                </div>
                            </figure>
                        }
                    })
                    .collect_view();
                view! {
                    <section id=g.clone() style="margin-bottom: 40px;">
                        <h2>{g.clone()}</h2>
                        <div style="display: flex; flex-wrap: wrap; gap: 20px; align-items: flex-start;">{figures}</div>
                    </section>
                }
            })
            .collect_view();
        view! {
            <div style="padding: 24px;">
                <h1>"tC4 design-system gallery, drawn by the Leptos port"</h1>
                <p>{count} " cases. Each loads in an iframe of " <code>"?case=<id>"</code> "."</p>
                <nav style="display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 24px;">{nav}</nav>
                {sections}
            </div>
        }
    };
    leptos::mount::mount_to(root, page).forget();
}
