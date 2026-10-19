//! Help and guides (S13, tC4 #519, `tC4:src/views/AccountMenu.jsx:229-289`): the help pages of
//! this build in a same-origin frame that fills the window. A new window would open in the
//! system browser and skip the internet switch, so the pages stay here. A link to another site
//! is the internet task `helpLink`, which opens the browser; Escape in the frame closes the
//! panel too. The page swap and the link rules are tC4's code, in `app/help-shim.js`.

use std::time::Duration;

use leptos::html;
use leptos::prelude::*;
use send_wrapper::SendWrapper;
use tc_api::account::WebUrl;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::app::internet::LinkKind;
use crate::app::web::use_app;
use crate::ds::{Attrs, Dismiss, Fill, LayerPlacement, Level, Shape, Size, Surface, Tone, Weight};
use crate::i18n::{keys, t};

#[wasm_bindgen(module = "/app/help-shim.js")]
extern "C" {
    /// Watches the frame's documents; answers the function that stops the watch.
    #[wasm_bindgen(js_name = watchHelp)]
    fn watch_help(
        frame: &web_sys::HtmlIFrameElement,
        open_link: &js_sys::Function,
        close: &js_sys::Function,
    ) -> js_sys::Function;
}

/// The help panel. `on_close` closes it and gives the focus back to the account button.
#[component]
pub fn HelpPanel(on_close: Callback<()>) -> impl IntoView {
    let actions = use_app().map(|ctx| ctx.actions);
    let frame = NodeRef::<html::Iframe>::new();
    Effect::new(move |_| {
        let Some(element) = frame.get() else {
            return;
        };
        let actions = actions.clone();
        let open_link = Closure::<dyn Fn(String)>::new(move |url: String| {
            if let (Some(actions), Ok(url)) = (actions.as_ref(), WebUrl::parse(&url)) {
                actions.open_door43_page(LinkKind::Help, url);
            }
        });
        let close = Closure::<dyn Fn()>::new(move || on_close.run(()));
        let stop = watch_help(
            &element,
            open_link.as_ref().unchecked_ref(),
            close.as_ref().unchecked_ref(),
        );
        let kept = SendWrapper::new((open_link, close, stop));
        on_cleanup(move || {
            let (open_link, close, stop) = kept.take();
            let _ = stop.call0(&JsValue::NULL);
            // The frame's Escape may be what closed the panel: the closures go once that call
            // has returned.
            set_timeout(move || drop((open_link, close)), Duration::ZERO);
        });
    });
    let content: ChildrenFn = std::sync::Arc::new(move || {
        let close = crate::ds::Action(crate::ds::ActionProps {
            weight: Weight::Soft,
            icon_only: true,
            shape: Shape::Square,
            size: Size::Sm,
            tone: Some(Tone::Neutral),
            title: Signal::derive(|| Some(t(keys::common::CLOSE, &[]))),
            on_click: Some(Callback::new(move |_| on_close.run(()))),
            attrs: Attrs::from([("data-testid", "help-close")]),
            style: Signal::from(crate::css! { "border-radius": "var(--radius-pill)" }),
            children: Some(Box::new(|| "✕".into_any())),
            ..Default::default()
        });
        let header = crate::css! {
            "display": "flex",
            "align-items": "center",
            "gap": 12,
            "padding": "10px 16px",
            "border-bottom": "1px solid var(--border)",
        };
        let heading = crate::css! {
            "flex": 1,
            "font-size": "var(--fs-ui)",
            "color": "var(--text-body)",
        };
        view! {
            <Surface fill=Fill::Card style=crate::css! { "height": "100%", "display": "flex", "flex-direction": "column" }>
                <div style=header.to_style()>
                    <strong style=heading.to_style()>{move || t(keys::account::HELP, &[])}</strong>
                    {close}
                </div>
                <iframe
                    node_ref=frame
                    title=move || t(keys::account::HELP, &[])
                    src="/help/index.html"
                    tabindex="0"
                    data-testid="help-frame"
                    style="flex: 1; width: 100%; border: 0;"
                ></iframe>
            </Surface>
        }
        .into_any()
    });
    crate::ds::Layer(crate::ds::LayerProps {
        level: Level::Overlay,
        placement: LayerPlacement::End,
        role: Some("dialog".to_owned()),
        label: Some(untrack(|| t(keys::account::HELP, &[]))),
        dismiss: Dismiss::parse("escape"),
        trap_focus: true,
        lock_scroll: true,
        on_dismiss: Signal::from(Some(Callback::new(move |_| on_close.run(())))),
        style: crate::css! { "width": "100vw" },
        attrs: Attrs::from([("data-testid", "help-panel")]),
        ..crate::ds::LayerProps::new(content)
    })
}
