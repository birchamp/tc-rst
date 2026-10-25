//! The export menu's presses (D2; tC4 `ExportMenu.jsx` and `exportFile`): `export/preflight`,
//! then `export/run` as a job, then the file. The decisions are `crate::app::print`'s.

use leptos::prelude::*;
use tc_api::export::{ExportedFile, Preflight, Producer, RunExport};
use tc_api::model::print::PageSetup;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use super::app_ctx::Actions;
use super::sources::api_error;
use super::{bridge, js};
use crate::app::jobs::JobEnd;
use crate::app::print::{
    ExportOf, ExportPanel, conflict_question, delivered_panel, failure_text, pdf_panel,
    prints_pictures, unfinished_panel,
};

/// tC4's `deliverFile`: a click on `<a download>`, so the browser or the shell saves it.
pub(crate) fn deliver(download: &str, file_name: &str) {
    let href = format!(
        "/download/{download}/{}",
        js_sys::encode_uri_component(file_name)
    );
    let Some(document) = js::window().ok().and_then(|w| w.document()) else {
        return;
    };
    let Some(link) = document
        .create_element("a")
        .ok()
        .and_then(|a| a.dyn_into::<web_sys::HtmlAnchorElement>().ok())
    else {
        return;
    };
    link.set_href(&href);
    link.set_download(file_name);
    link.click();
}

impl Actions {
    /// A press of an export item. The press has one request id, so "Export anyway" after the
    /// conflicts question sends the same request.
    pub fn export_press(
        &self,
        panel: RwSignal<ExportPanel>,
        of: &ExportOf,
        producer: Producer,
        setup: PageSetup,
    ) {
        if panel.get_untracked() == ExportPanel::Running {
            return;
        }
        let run = of.run(js::random_request_id(), producer, setup);
        panel.set(ExportPanel::Running);
        let this = self.clone();
        spawn_local(async move {
            let asked = Preflight {
                at: run.at.clone(),
                subject: run.subject,
            };
            match this.runtime().api().call(&asked).await.map_err(api_error) {
                Ok(view) => match conflict_question(view.verses) {
                    Some(question) => {
                        panel.try_set(ExportPanel::Ask {
                            question,
                            run: Box::new(run),
                        });
                    }
                    None => this.export_run(panel, run),
                },
                Err(error) => {
                    panel.try_set(ExportPanel::Failed(failure_text(&error)));
                }
            }
        });
    }

    /// "Export anyway", or a press with nothing in conflict.
    pub fn export_run(&self, panel: RwSignal<ExportPanel>, run: RunExport) {
        panel.try_set(ExportPanel::Running);
        let this = self.clone();
        spawn_local(async move {
            let pictures = prints_pictures(&run);
            let job = match this.runtime().api().call(&run).await.map_err(api_error) {
                Ok(job) => job,
                Err(error) => {
                    panel.try_set(ExportPanel::Failed(failure_text(&error)));
                    return;
                }
            };
            let shown = match this.follow::<RunExport>(job, |_| {}, || true).await {
                JobEnd::Done(exported) => match &exported.file {
                    ExportedFile::Download {
                        download,
                        file_name,
                    } => {
                        deliver(download.as_str(), file_name);
                        delivered_panel(&exported, bridge::has_download_done())
                    }
                    ExportedFile::Print { .. } => {
                        let answer = bridge::print_pdf(&format!("/print/{}", job.get())).await;
                        pdf_panel(answer, &exported, pictures)
                    }
                },
                JobEnd::Failed(error) => ExportPanel::Failed(failure_text(&error)),
                end => match unfinished_panel(&end) {
                    Some(shown) => shown,
                    None => return,
                },
            };
            panel.try_set(shown);
        });
    }
}
