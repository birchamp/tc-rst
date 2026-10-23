//! tC4's view-error callout (`tC4:src/App.jsx:177-190`): what the page shows in place of a
//! screen that panicked, after the reload (APP-DESIGN 6.8, AD22). `boot.js` draws the same DOM
//! as static elements after three panics; the no-loss smoke compares the two.

use leptos::prelude::*;

use crate::attrs::Attrs;
use crate::ds::{Button, ButtonVariant, Callout, CalloutTone, Size};
use crate::i18n::{keys, t};

#[component]
pub fn ViewError(message: String, on_home: Callback<()>) -> impl IntoView {
    let home = Callback::new(move |_: web_sys::MouseEvent| on_home.run(()));
    view! {
        <div style="flex:1;padding:24px">
            <Callout
                tone=CalloutTone::Warn
                attrs=Attrs::from([("role", "alert"), ("data-testid", "view-error")])
                style=crate::css! { "overflow-wrap": "anywhere" }
            >
                <strong>{move || t(keys::app::VIEW_ERROR, &[])}</strong>
                " "
                {message}
                " "
                <Button
                    size=Size::Sm
                    variant=ButtonVariant::Outline
                    attrs=Attrs::from([("data-testid", "view-error-home")])
                    on_click=home
                >
                    {move || t(keys::app::BACK_TO_HOME, &[])}
                </Button>
            </Callout>
        </div>
    }
}
