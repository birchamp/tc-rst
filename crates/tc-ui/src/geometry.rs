//! The arithmetic the design system does in JavaScript: concentric corners (Surface) and
//! anchored placement with a flip (Layer). Pure functions, so the host tests them.

use crate::css::js_number;
use crate::tokens::{Border, Corner, Edge, LayerAlign, Pad, Radius, Side};

/// A Surface's padding as `[block, inline]` px (`padPair` in Surface.jsx): a number, an inset
/// token, or the first two numbers of a CSS shorthand.
pub fn pad_pair(pad: &Pad) -> Option<[f64; 2]> {
    match pad {
        Pad::Px(n) => Some([*n, *n]),
        Pad::Inset(i) => Some([i.px(), i.px()]),
        Pad::Css(s) => {
            let n = numbers(s);
            match n.as_slice() {
                [] => None,
                [a] => Some([*a, *a]),
                [a, b, ..] => Some([*a, *b]),
            }
        }
    }
}

/// `String(pad).match(/-?[\d.]+/g).map(Number)`.
fn numbers(s: &str) -> Vec<f64> {
    let num = |c: char| c.is_ascii_digit() || c == '.';
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let signed = chars[i] == '-' && chars.get(i + 1).is_some_and(|c| num(*c));
        if signed || num(chars[i]) {
            let start = i;
            i += 1;
            while i < chars.len() && num(chars[i]) {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            // Number(".") and Number("1.2.3") are NaN in JavaScript, and NaN never binds.
            out.push(text.parse().unwrap_or(f64::NAN));
        } else {
            i += 1;
        }
    }
    out
}

fn corner_px(c: Corner) -> Option<f64> {
    Some(match c {
        Corner::None => 0.0,
        Corner::Xs => 4.0,
        Corner::Chip => 7.0,
        Corner::Sm => 8.0,
        Corner::Input => 9.0,
        Corner::Md => 10.0,
        Corner::Lg => 12.0,
        Corner::Xl => 14.0,
        Corner::X2l => 16.0,
        Corner::Pill | Corner::Inner => return None,
    })
}

/// The radius token a corner keyword resolves to.
pub fn corner_var(c: Corner) -> &'static str {
    match c {
        Corner::None => "0",
        Corner::Xs => "var(--radius-xs)",
        Corner::Chip => "var(--radius-chip)",
        Corner::Sm => "var(--radius-sm)",
        Corner::Input => "var(--radius-input)",
        Corner::Md => "var(--radius-md)",
        Corner::Lg => "var(--radius-lg)",
        Corner::Xl => "var(--radius-xl)",
        Corner::X2l => "var(--radius-2xl)",
        Corner::Pill => "var(--radius-pill)",
        Corner::Inner => "var(--radius-inner, var(--radius-sm))",
    }
}

fn stroke(border: &Border) -> (&'static str, f64) {
    match border {
        Border::Edge(Edge::None) => ("0px", 0.0),
        Border::Edge(Edge::Hair) => ("var(--stroke-hair)", 1.0),
        Border::Edge(Edge::Line | Edge::Input | Edge::Strong | Edge::Tone) => {
            ("var(--stroke)", 1.0)
        }
        Border::Edge(Edge::Choice | Edge::Dashed | Edge::DashedTone) => {
            ("var(--stroke-selected)", 1.5)
        }
        // A raw border has no stroke token: Surface.jsx counts it as 0px.
        Border::Css(_) => ("0px", 0.0),
    }
}

/// The `--radius-inner` a Surface publishes for a child with `radius="inner"`, when the
/// concentric rule `r = R - b - d` binds (Surface.jsx explains the derivation).
pub fn inner_radius(radius: &Radius, border: &Border, pad: Option<&Pad>) -> Option<String> {
    let corner = match radius {
        Radius::Corner(c) => *c,
        Radius::Css(_) => return None,
    };
    if corner == Corner::Pill {
        return Some("var(--radius-pill)".to_string());
    }
    let outer = corner_px(corner)?;
    let [block, inline] = pad_pair(pad?)?;
    let (b_var, b_px) = stroke(border);
    let binds = outer - b_px - inline > 0.0 && outer - b_px - block > 0.0;
    if !binds {
        return None;
    }
    let r = corner_var(corner);
    let rx = format!("calc({r} - {b_var} - {}px)", js_number(inline));
    if block == inline {
        Some(rx)
    } else {
        Some(format!(
            "{rx} / calc({r} - {b_var} - {}px)",
            js_number(block)
        ))
    }
}

/// A box in window coordinates, as `getBoundingClientRect` gives it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn right(&self) -> f64 {
        self.left + self.width
    }
    pub fn bottom(&self) -> f64 {
        self.top + self.height
    }
}

/// Where an anchored panel goes, and the side it ended up on after the flip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub left: f64,
    pub top: f64,
    pub side: Side,
}

/// The room a panel keeps from the window's edges.
pub const GUTTER: f64 = 8.0;

/// `place()` in Layer.jsx: the panel's position beside its anchor, flipped to the opposite
/// side when it does not fit and the other side does, then kept on screen. `side` and `align`
/// are logical and resolved against the anchor's direction.
pub fn place(
    rect: Rect,
    panel: (f64, f64),
    side: Side,
    align: LayerAlign,
    offset: f64,
    rtl: bool,
    viewport: (f64, f64),
) -> Placed {
    let (vw, vh) = viewport;
    let (pw, ph) = panel;
    let room = |s: Side| match s {
        Side::Bottom => vh - rect.bottom() - offset - GUTTER,
        Side::Top => rect.top - offset - GUTTER,
        Side::Start => (if rtl { vw - rect.right() } else { rect.left }) - offset - GUTTER,
        Side::End => (if rtl { rect.left } else { vw - rect.right() }) - offset - GUTTER,
    };
    let need = if matches!(side, Side::Bottom | Side::Top) {
        ph
    } else {
        pw
    };
    let mut s = side;
    if room(s) < need && room(s.opposite()) >= need {
        s = s.opposite();
    }
    let clamp = |v: f64, size: f64, max: f64| GUTTER.max(v.min(max - size - GUTTER));
    let (left, top);
    if matches!(s, Side::Bottom | Side::Top) {
        let t = if s == Side::Bottom {
            rect.bottom() + offset
        } else {
            rect.top - offset - ph
        };
        let start_edge = if rtl { rect.right() - pw } else { rect.left };
        let end_edge = if rtl { rect.left } else { rect.right() - pw };
        let l = match align {
            LayerAlign::End => end_edge,
            LayerAlign::Center => rect.left + rect.width / 2.0 - pw / 2.0,
            LayerAlign::Start => start_edge,
        };
        left = clamp(l, pw, vw);
        top = GUTTER.max(t.min(vh - ph - GUTTER));
    } else {
        let before = if rtl {
            rect.right() + offset
        } else {
            rect.left - offset - pw
        };
        let after = if rtl {
            rect.left - offset - pw
        } else {
            rect.right() + offset
        };
        let l = if s == Side::Start { before } else { after };
        let t = match align {
            LayerAlign::End => rect.bottom() - ph,
            LayerAlign::Center => rect.top + rect.height / 2.0 - ph / 2.0,
            LayerAlign::Start => rect.top,
        };
        top = clamp(t, ph, vh);
        left = GUTTER.max(l.min(vw - pw - GUTTER));
    }
    Placed { left, top, side: s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_read_like_surface_jsx() {
        assert_eq!(pad_pair(&Pad::from("md")), Some([14.0, 14.0]));
        assert_eq!(pad_pair(&Pad::Px(6.0)), Some([6.0, 6.0]));
        assert_eq!(pad_pair(&Pad::from("10px 12px")), Some([10.0, 12.0]));
        assert_eq!(pad_pair(&Pad::from("0 16px")), Some([0.0, 16.0]));
        assert_eq!(pad_pair(&Pad::from("38px 24px 4px")), Some([38.0, 24.0]));
        assert_eq!(pad_pair(&Pad::from("var(--x)")), None);
    }

    #[test]
    fn the_concentric_radius_binds_only_when_positive() {
        let line = Border::Edge(Edge::Line);
        // 2xl (16) - line (1) - xs (8) = 7 > 0: binds, as calc() over the tokens.
        assert_eq!(
            inner_radius(&Radius::Corner(Corner::X2l), &line, Some(&Pad::from("xs"))),
            Some("calc(var(--radius-2xl) - var(--stroke) - 8px)".to_string())
        );
        // Per-axis insets give an ellipse.
        assert_eq!(
            inner_radius(&Radius::Corner(Corner::X2l), &line, Some(&Pad::from("6px 10px"))),
            Some("calc(var(--radius-2xl) - var(--stroke) - 10px) / calc(var(--radius-2xl) - var(--stroke) - 6px)".to_string())
        );
        // lg (12) - 0 - md (14) < 0: the child's corner is free.
        assert_eq!(
            inner_radius(
                &Radius::Corner(Corner::Lg),
                &Border::default(),
                Some(&Pad::from("md"))
            ),
            None
        );
        // No pad, no rule; a pill is always a pill.
        assert_eq!(
            inner_radius(&Radius::Corner(Corner::Lg), &Border::default(), None),
            None
        );
        assert_eq!(
            inner_radius(&Radius::Corner(Corner::Pill), &Border::default(), None),
            Some("var(--radius-pill)".to_string())
        );
    }

    #[test]
    fn a_panel_flips_when_only_the_other_side_has_room() {
        let anchor = Rect {
            left: 100.0,
            top: 360.0,
            width: 80.0,
            height: 30.0,
        };
        let p = place(
            anchor,
            (200.0, 120.0),
            Side::Bottom,
            LayerAlign::Start,
            6.0,
            false,
            (640.0, 400.0),
        );
        assert_eq!(p.side, Side::Top);
        assert_eq!(p.top, 360.0 - 6.0 - 120.0);
        assert_eq!(p.left, 100.0);
    }

    #[test]
    fn start_and_end_follow_the_anchor_direction() {
        let anchor = Rect {
            left: 300.0,
            top: 100.0,
            width: 80.0,
            height: 30.0,
        };
        let ltr = place(
            anchor,
            (100.0, 40.0),
            Side::Bottom,
            LayerAlign::End,
            6.0,
            false,
            (640.0, 400.0),
        );
        let rtl = place(
            anchor,
            (100.0, 40.0),
            Side::Bottom,
            LayerAlign::End,
            6.0,
            true,
            (640.0, 400.0),
        );
        assert_eq!(ltr.left, 380.0 - 100.0);
        assert_eq!(rtl.left, 300.0);
        let beside = place(
            anchor,
            (100.0, 40.0),
            Side::End,
            LayerAlign::Center,
            7.0,
            true,
            (640.0, 400.0),
        );
        assert_eq!(beside.left, 300.0 - 7.0 - 100.0);
        assert_eq!(beside.top, 100.0 + 15.0 - 20.0);
    }

    #[test]
    fn the_cross_axis_shifts_to_stay_on_screen() {
        let anchor = Rect {
            left: 600.0,
            top: 10.0,
            width: 30.0,
            height: 30.0,
        };
        let p = place(
            anchor,
            (200.0, 100.0),
            Side::Bottom,
            LayerAlign::Start,
            6.0,
            false,
            (640.0, 400.0),
        );
        assert_eq!(p.left, 640.0 - 200.0 - GUTTER);
        assert_eq!(p.side, Side::Bottom);
    }
}
