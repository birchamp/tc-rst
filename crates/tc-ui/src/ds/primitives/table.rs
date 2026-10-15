//! Table.jsx: a real `<table>`: caption, column headers with `scope`, row association and
//! per-column alignment. Cells render whatever the column's `cell` returns; the table owns
//! structure and nothing about appearance beyond the header treatment and the row rule.

use std::sync::Arc;

use leptos::ev::KeyboardEvent;
use leptos::prelude::*;

use crate::css::CssValue;
use crate::ds::{Attrs, ColumnAlign, Css, Text, TextRole, spread};

/// A row of a Table: its fields by column key, and its identity. Equality tells a row whose
/// data changed from one that only moved.
pub trait Record: Clone + PartialEq + Send + Sync + 'static {
    /// The text of the field `key`, for the default cell.
    fn field(&self, key: &str) -> Option<String>;
    /// The row's key when the table has no `row_key`: its `id` field.
    fn id(&self) -> Option<String> {
        self.field("id")
    }
}

/// Renders one cell of a row.
pub type Cell<R> = Arc<dyn Fn(&R, usize) -> AnyView + Send + Sync>;

/// One column of a Table.
pub struct Column<R> {
    pub key: String,
    pub header: String,
    pub align: ColumnAlign,
    pub width: Option<CssValue>,
    /// Let the column's text wrap; columns do not wrap by default.
    pub wrap: bool,
    /// The first column is each row's name to a screen reader unless this is false.
    pub row_header: bool,
    pub cell: Option<Cell<R>>,
}

impl<R> Clone for Column<R> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            header: self.header.clone(),
            align: self.align,
            width: self.width.clone(),
            wrap: self.wrap,
            row_header: self.row_header,
            cell: self.cell.clone(),
        }
    }
}

impl<R> Column<R> {
    pub fn new(key: impl Into<String>, header: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            header: header.into(),
            align: ColumnAlign::Start,
            width: None,
            wrap: false,
            row_header: true,
            cell: None,
        }
    }
}

fn cell_css<R>(col: &Column<R>, header: bool) -> Css {
    let mut s = css! {
        "padding": "9px 12px",
        "text-align": match col.align {
            ColumnAlign::Start => "start",
            ColumnAlign::Center => "center",
            ColumnAlign::End | ColumnAlign::Numeric => "end",
        },
        "vertical-align": if header { "bottom" } else { "middle" },
    };
    s.set_opt("width", col.width.clone());
    s.set("white-space", if col.wrap { "normal" } else { "nowrap" });
    s.set(
        "border-bottom",
        "var(--stroke-hair) solid var(--border-hair)",
    );
    if col.align == ColumnAlign::Numeric {
        s.set("font-variant-numeric", "tabular-nums");
    }
    s
}

/// Keys a row: the caller's `row_key`, else its `id`, else its index.
pub type RowKey<R> = Arc<dyn Fn(&R, usize) -> String + Send + Sync>;

#[component]
pub fn Table<R: Record>(
    #[prop(optional)] columns: Vec<Column<R>>,
    #[prop(optional, into)] rows: Signal<Vec<R>>,
    #[prop(optional, into)] caption: Option<String>,
    #[prop(optional)] caption_visible: bool,
    #[prop(optional)] row_key: Option<RowKey<R>>,
    #[prop(optional)] on_row_click: Option<Callback<(R, usize)>>,
    #[prop(optional, into)] selected_key: Signal<Option<String>>,
    /// Shown in one full-width cell when there are no rows.
    #[prop(optional)]
    empty: Option<ViewFn>,
    #[prop(optional)] style: Css,
    #[prop(optional)] attrs: Attrs,
) -> impl IntoView {
    let table_style = css! {
        "width": "100%",
        "border-collapse": "collapse",
        "border-spacing": 0,
        "font-family": "var(--font-ui)",
    }
    .merge(&style);
    let caption = caption.filter(|c| !c.is_empty()).map(|c| {
        if caption_visible {
            let s = css! { "text-align": "start", "padding": "0 0 10px" };
            view! {
                <caption style=s.to_style()>
                    <Text role=TextRole::Overline>{c}</Text>
                </caption>
            }
            .into_any()
        } else {
            let s = css! {
                "position": "absolute",
                "width": 1,
                "height": 1,
                "overflow": "hidden",
                "clip": "rect(0 0 0 0)",
                "clip-path": "inset(50%)",
                "white-space": "nowrap",
            };
            view! { <caption style=s.to_style()>{c}</caption> }.into_any()
        }
    });
    let head = columns
        .iter()
        .map(|c| {
            let s = cell_css(c, true).merge(&css! { "border-bottom": "var(--stroke) solid var(--line)", "background": "transparent" });
            let header = c.header.clone();
            view! {
                <th scope="col" style=s.to_style()>
                    <Text role=TextRole::Label>{header}</Text>
                </th>
            }
        })
        .collect_view();
    let columns = StoredValue::new(columns);
    // Each row as drawn: its key, its index and its record. Rows are keyed as React keys them,
    // so a row whose key stays keeps its `<tr>` (and the keyboard focus on it) while the
    // selection or the data around it changes.
    let keyed = Memo::new(move |_| {
        rows.with(|rows| {
            rows.iter()
                .enumerate()
                .map(|(i, r)| {
                    let k = row_key
                        .as_ref()
                        .map(|f| f(r, i))
                        .or_else(|| r.id())
                        .unwrap_or_else(|| i.to_string());
                    (k, i, r.clone())
                })
                .collect::<Vec<_>>()
        })
    });
    let has_empty = empty.is_some();
    let show_empty = Memo::new(move |_| has_empty && keyed.with(Vec::is_empty));
    let row = move |k: String| {
        let current = {
            let k = k.clone();
            Memo::new(move |_| {
                keyed.with(|rows| {
                    rows.iter()
                        .find(|(rk, _, _)| *rk == k)
                        .map(|(_, i, r)| (*i, r.clone()))
                })
            })
        };
        let on = Memo::new(move |_| selected_key.with(|s| s.as_ref() == Some(&k)));
        let clickable = on_row_click.is_some();
        let style = move || {
            let mut s = Css::new();
            s.set_opt("cursor", clickable.then_some("pointer"));
            s.set_opt("background", on.get().then_some("var(--tone-soft)"));
            s.to_style()
        };
        let activate = move || {
            if let (Some(cb), Some((i, r))) = (on_row_click, current.get_untracked()) {
                cb.run((r, i));
            }
        };
        let cells = columns.with_value(|cols| {
            cols.iter()
                .enumerate()
                .map(|(ci, c)| {
                    let col = c.clone();
                    let content = move || {
                        current.with(|cur| {
                            cur.as_ref().map(|(i, r)| match &col.cell {
                                Some(cell) => cell(r, *i),
                                None => {
                                    let text = r.field(&col.key);
                                    view! { <Text role=TextRole::Ui>{text}</Text> }.into_any()
                                }
                            })
                        })
                    };
                    if ci == 0 && c.row_header {
                        let s = cell_css(c, false).merge(&css! { "font-weight": "inherit" });
                        view! { <th scope="row" style=s.to_style()>{content}</th> }.into_any()
                    } else {
                        view! { <td style=cell_css(c, false).to_style()>{content}</td> }.into_any()
                    }
                })
                .collect_view()
        });
        view! {
            <tr
                data-i=clickable.then_some("row")
                data-selected=move || on.get().then_some("true")
                aria-selected=move || {
                    selected_key.with(|s| s.as_ref().map(|_| if on.get() { "true" } else { "false" }))
                }
                tabindex=clickable.then_some("0")
                on:click=move |_| activate()
                on:keydown=move |e: KeyboardEvent| {
                    if clickable && (e.key() == "Enter" || e.key() == " ") {
                        e.prevent_default();
                        activate();
                    }
                }
                style=style
            >
                {cells}
            </tr>
        }
    };
    let body = move || {
        if show_empty.get() {
            let ncols = columns.with_value(Vec::len);
            let empty = empty.clone().map(|e| e.run());
            view! {
                <tr>
                    <td colspan=ncols.to_string() style="padding: 22px 12px; text-align: center;">{empty}</td>
                </tr>
            }
            .into_any()
        } else {
            view! { <For each=move || keyed.get() key=|(k, _, _)| k.clone() children=move |(k, _, _)| row(k) /> }
                .into_any()
        }
    };
    view! {
        <table style=table_style.to_style() {..spread(&attrs)}>
            {caption}
            <thead>
                <tr>{head}</tr>
            </thead>
            <tbody>{body}</tbody>
        </table>
    }
}
