//! The design system's keyword props as enums.
//!
//! Each tC4 component takes these as strings (`tone="accent"`, `weight="fill"`). Here every
//! such prop is an enum whose `as_str` is the string tC4 writes, and whose `parse` reads it at
//! the boundary (the gallery's JSON, later the app's own data).

macro_rules! keyword {
    (
        $(#[$m:meta])*
        pub enum $name:ident { $( $(#[$vm:meta])* $var:ident = $s:literal ),+ $(,)? }
    ) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $( $(#[$vm])* $var ),+ }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [$name] = &[$( $name::$var ),+];

            /// The string tC4 writes for this value.
            pub const fn as_str(self) -> &'static str {
                match self { $( Self::$var => $s ),+ }
            }

            /// The value tC4 writes as `s`, if any.
            pub fn parse(s: &str) -> Option<Self> {
                match s { $( $s => Some(Self::$var), )+ _ => None }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl Keyword for $name {
            fn from_keyword(s: &str) -> Option<Self> {
                Self::parse(s)
            }
        }
    };
}

/// A prop whose value is one of a fixed set of strings.
pub trait Keyword: Sized {
    fn from_keyword(s: &str) -> Option<Self>;
}

keyword! {
    /// `data-tone`: what a thing means. tokens/tone.css maps each to nine colour slots.
    #[derive(Default)]
    pub enum Tone {
        Neutral = "neutral",
        #[default]
        Accent = "accent",
        Ocean = "ocean",
        Cultivate = "cultivate",
        Valid = "valid",
        Warn = "warn",
        Invalid = "invalid",
    }
}

keyword! {
    /// `data-on`: the ground a container declares for everything inside it.
    pub enum Ground { Light = "light", Dark = "dark" }
}

keyword! {
    /// The three control sizes.
    #[derive(Default)]
    pub enum Size { Sm = "sm", #[default] Md = "md", Lg = "lg" }
}

keyword! {
    /// `data-i`: the interaction role whose states tokens/states.css draws.
    pub enum Interactive {
        Fill = "fill",
        Outline = "outline",
        Quiet = "quiet",
        Text = "text",
        Choice = "choice",
        Card = "card",
        Row = "row",
        Field = "field",
    }
}

keyword! {
    /// Surface `fill`.
    #[derive(Default)]
    pub enum Fill {
        #[default]
        None = "none",
        Solid = "solid",
        Soft = "soft",
        Card = "card",
        App = "app",
        Muted = "muted",
        Quiet = "quiet",
        Paper = "paper",
    }
}

keyword! {
    /// Surface `border` keywords. A raw CSS border is [`Border::Css`].
    #[derive(Default)]
    pub enum Edge {
        #[default]
        None = "none",
        Hair = "hair",
        Line = "line",
        Input = "input",
        Strong = "strong",
        Tone = "tone",
        Choice = "choice",
        Dashed = "dashed",
        DashedTone = "dashed-tone",
    }
}

/// Surface `border`: a keyword, or a CSS border that passes through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Border {
    Edge(Edge),
    Css(String),
}

impl Default for Border {
    fn default() -> Self {
        Self::Edge(Edge::None)
    }
}

impl From<Edge> for Border {
    fn from(e: Edge) -> Self {
        Self::Edge(e)
    }
}

impl Border {
    pub fn parse(s: &str) -> Self {
        Edge::parse(s).map_or_else(|| Self::Css(s.to_string()), Self::Edge)
    }
}

keyword! {
    /// Surface `radius` keywords. A CSS length is [`Radius::Css`].
    #[derive(Default)]
    pub enum Corner {
        None = "none",
        Xs = "xs",
        Chip = "chip",
        Sm = "sm",
        Input = "input",
        Md = "md",
        #[default]
        Lg = "lg",
        Xl = "xl",
        X2l = "2xl",
        Pill = "pill",
        /// The concentric radius the enclosing Surface publishes.
        Inner = "inner",
    }
}

/// Surface `radius`: a keyword, or a CSS length that passes through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Radius {
    Corner(Corner),
    Css(String),
}

impl Default for Radius {
    fn default() -> Self {
        Self::Corner(Corner::Lg)
    }
}

impl From<Corner> for Radius {
    fn from(c: Corner) -> Self {
        Self::Corner(c)
    }
}

impl Radius {
    /// `resolveRadius` in Surface.jsx: a keyword, or a string that starts like a CSS length
    /// (`0`, a digit or a dot, `calc(`, `var(`, `clamp(`, `min(`, `max(`). Anything else is a
    /// typo, `None`; the caller falls back to `lg`.
    pub fn parse(s: &str) -> Option<Self> {
        if let Some(c) = Corner::parse(s) {
            return Some(Self::Corner(c));
        }
        let t = s.trim();
        let length = t == "0"
            || t.starts_with(|c: char| c.is_ascii_digit() || c == '.')
            || ["calc(", "var(", "clamp(", "min(", "max("]
                .iter()
                .any(|f| t.starts_with(f));
        length.then(|| Self::Css(s.to_string()))
    }
}

keyword! {
    /// Surface `elevation` keywords. A raw `box-shadow` is [`Elevation::Css`].
    #[derive(Default)]
    pub enum Shadow {
        #[default]
        None = "none",
        Chip = "chip",
        Raised = "raised",
        Card = "card",
        Hover = "hover",
        Hero = "hero",
        Page = "page",
        Modal = "modal",
        Drawer = "drawer",
        Focus = "focus",
    }
}

/// Surface `elevation`: a keyword, or a `box-shadow` that passes through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Elevation {
    Shadow(Shadow),
    Css(String),
}

impl Default for Elevation {
    fn default() -> Self {
        Self::Shadow(Shadow::None)
    }
}

impl From<Shadow> for Elevation {
    fn from(s: Shadow) -> Self {
        Self::Shadow(s)
    }
}

impl Elevation {
    pub fn parse(s: &str) -> Self {
        Shadow::parse(s).map_or_else(|| Self::Css(s.to_string()), Self::Shadow)
    }
}

keyword! {
    /// Surface `pad` keywords.
    pub enum Inset { None = "none", Xs = "xs", Sm = "sm", Md = "md", Lg = "lg", Xl = "xl" }
}

impl Inset {
    /// The inset in px.
    pub const fn px(self) -> f64 {
        match self {
            Self::None => 0.0,
            Self::Xs => 8.0,
            Self::Sm => 12.0,
            Self::Md => 14.0,
            Self::Lg => 22.0,
            Self::Xl => 24.0,
        }
    }
}

/// Surface `pad`: a keyword, a number of px, or a CSS padding shorthand.
#[derive(Debug, Clone, PartialEq)]
pub enum Pad {
    Inset(Inset),
    Px(f64),
    Css(String),
}

impl From<Inset> for Pad {
    fn from(i: Inset) -> Self {
        Self::Inset(i)
    }
}

impl From<f64> for Pad {
    fn from(n: f64) -> Self {
        Self::Px(n)
    }
}

impl From<&str> for Pad {
    fn from(s: &str) -> Self {
        Inset::parse(s).map_or_else(|| Self::Css(s.to_string()), Self::Inset)
    }
}

keyword! {
    /// Text `role`: what a string is.
    #[derive(Default)]
    pub enum TextRole {
        Display = "display",
        H1 = "h1",
        H2 = "h2",
        H3 = "h3",
        Title = "title",
        TitleSm = "titleSm",
        Strong = "strong",
        Body = "body",
        #[default]
        Ui = "ui",
        Caption = "caption",
        CaptionStrong = "captionStrong",
        Overline = "overline",
        Label = "label",
        LabelNum = "labelNum",
        LabelMicro = "labelMicro",
        Meta = "meta",
        Scripture = "scripture",
        ScriptureMd = "scriptureMd",
        ScriptureSm = "scriptureSm",
        Quote = "quote",
    }
}

keyword! {
    /// Text `tone`: a colour override of the role's own.
    #[derive(Default)]
    pub enum TextTone {
        #[default]
        Default = "default",
        Strong = "strong",
        Muted = "muted",
        Faint = "faint",
        Accent = "accent",
        Tone = "tone",
        Inverse = "inverse",
        Scripture = "scripture",
    }
}

keyword! {
    /// The script of a run of scripture.
    #[derive(Default)]
    pub enum Script {
        #[default]
        Latin = "latin",
        Greek = "greek",
        Hebrew = "hebrew",
        Arabic = "arabic",
        Nastaliq = "nastaliq",
    }
}

impl Script {
    /// Scripts written right to left.
    pub const fn is_rtl(self) -> bool {
        matches!(self, Self::Hebrew | Self::Arabic | Self::Nastaliq)
    }
}

keyword! {
    /// Text `measure` keywords.
    pub enum MeasureToken { Read = "read", Prose = "prose", Narrow = "narrow" }
}

/// Text `measure`: a reading measure token, or a CSS `max-width`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Measure {
    Token(MeasureToken),
    Css(String),
}

keyword! {
    /// Stack `direction`.
    #[derive(Default)]
    pub enum Direction {
        Row = "row",
        #[default]
        Column = "column",
        RowReverse = "row-reverse",
        ColumnReverse = "column-reverse",
    }
}

keyword! {
    /// Stack `align`.
    pub enum Align { Start = "start", Center = "center", End = "end", Stretch = "stretch", Baseline = "baseline" }
}

impl Align {
    pub const fn flex(self) -> &'static str {
        match self {
            Self::Start => "flex-start",
            Self::Center => "center",
            Self::End => "flex-end",
            Self::Stretch => "stretch",
            Self::Baseline => "baseline",
        }
    }
}

keyword! {
    /// Stack `justify`.
    pub enum Justify { Start = "start", Center = "center", End = "end", Between = "between" }
}

impl Justify {
    pub const fn flex(self) -> &'static str {
        match self {
            Self::Start => "flex-start",
            Self::Center => "center",
            Self::End => "flex-end",
            Self::Between => "space-between",
        }
    }
}

/// Stack `columns`: a count of equal columns, or a `grid-template-columns` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Columns {
    Count(u32),
    Template(String),
}

keyword! {
    /// Action `weight`: how loud it is.
    #[derive(Default)]
    pub enum Weight {
        #[default]
        Fill = "fill",
        Soft = "soft",
        Outline = "outline",
        Quiet = "quiet",
        Text = "text",
        Row = "row",
    }
}

keyword! {
    /// Action `shape`.
    #[derive(Default)]
    pub enum Shape { #[default] Pill = "pill", Block = "block", Square = "square" }
}

keyword! {
    /// Choice `mark`.
    #[derive(Default)]
    pub enum Mark { #[default] Check = "check", Radio = "radio", Switch = "switch" }
}

keyword! {
    /// Field `placement`: where the label sits.
    #[derive(Default)]
    pub enum Placement { #[default] Above = "above", Start = "start", End = "end" }
}

keyword! {
    /// Input `as`: the native element.
    #[derive(Default)]
    pub enum NativeInput { #[default] Input = "input", Textarea = "textarea", Select = "select" }
}

keyword! {
    /// Input `variant`.
    #[derive(Default)]
    pub enum InputVariant { #[default] Ui = "ui", Scripture = "scripture" }
}

keyword! {
    /// Layer `level`: the stacking level, `--z-<level>`.
    #[derive(Default)]
    pub enum Level {
        Base = "base",
        Raised = "raised",
        Header = "header",
        #[default]
        Overlay = "overlay",
        Popover = "popover",
        Toast = "toast",
        Tooltip = "tooltip",
    }
}

keyword! {
    /// Layer `placement`.
    #[derive(Default)]
    pub enum LayerPlacement {
        #[default]
        Center = "center",
        Start = "start",
        End = "end",
        BottomStart = "bottom-start",
        BottomEnd = "bottom-end",
        Anchor = "anchor",
    }
}

keyword! {
    /// The side of an anchor a layer opens on.
    #[derive(Default)]
    pub enum Side { #[default] Bottom = "bottom", Top = "top", Start = "start", End = "end" }
}

impl Side {
    pub const fn opposite(self) -> Self {
        match self {
            Self::Bottom => Self::Top,
            Self::Top => Self::Bottom,
            Self::Start => Self::End,
            Self::End => Self::Start,
        }
    }
}

keyword! {
    /// The cross-axis alignment of an anchored layer.
    #[derive(Default)]
    pub enum LayerAlign { #[default] Start = "start", Center = "center", End = "end" }
}

keyword! {
    /// Layer `scrim` keywords.
    pub enum ScrimToken { Modal = "modal", Drawer = "drawer" }
}

/// Layer `scrim`: a token, or a CSS background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scrim {
    Token(ScrimToken),
    Css(String),
}

impl Scrim {
    pub fn parse(s: &str) -> Self {
        ScrimToken::parse(s).map_or_else(|| Self::Css(s.to_string()), Self::Token)
    }
}

keyword! {
    /// Layer `navigate`: roving focus with the arrow keys.
    pub enum Navigate { Vertical = "vertical", Horizontal = "horizontal" }
}

keyword! {
    /// Progress `shape`.
    #[derive(Default)]
    pub enum ProgressShape { #[default] Bar = "bar", Ring = "ring" }
}

keyword! {
    /// Rule and Divider `orientation`.
    #[derive(Default)]
    pub enum Orientation { #[default] Horizontal = "horizontal", Vertical = "vertical" }
}

keyword! {
    /// Rule `weight`.
    #[derive(Default)]
    pub enum RuleWeight { #[default] Hair = "hair", Line = "line" }
}

keyword! {
    /// Switcher `indicator`: how the selection is marked.
    #[derive(Default)]
    pub enum Indicator { #[default] Underline = "underline", Pill = "pill", None = "none" }
}

keyword! {
    /// Table column `align`.
    #[derive(Default)]
    pub enum ColumnAlign { #[default] Start = "start", Center = "center", End = "end", Numeric = "numeric" }
}

keyword! {
    /// Badge `tone`.
    #[derive(Default)]
    pub enum BadgeTone {
        #[default]
        Accent = "accent",
        AccentSoft = "accentSoft",
        Cultivate = "cultivate",
        Ocean = "ocean",
        Neutral = "neutral",
        Valid = "valid",
        Warn = "warn",
    }
}

keyword! {
    /// Badge `size`.
    #[derive(Default)]
    pub enum BadgeSize { Sm = "sm", #[default] Md = "md" }
}

keyword! {
    /// Button `variant`.
    #[derive(Default)]
    pub enum ButtonVariant {
        #[default]
        Primary = "primary",
        Ocean = "ocean",
        Secondary = "secondary",
        Outline = "outline",
        Ghost = "ghost",
        Valid = "valid",
        Danger = "danger",
    }
}

keyword! {
    /// IconButton `variant`.
    #[derive(Default)]
    pub enum IconButtonVariant { #[default] Outline = "outline", Muted = "muted", Plain = "plain" }
}

keyword! {
    /// Overline `tone`.
    #[derive(Default)]
    pub enum OverlineTone { #[default] Muted = "muted", Accent = "accent", Inverse = "inverse" }
}

keyword! {
    /// StatusDot `status`.
    #[derive(Default)]
    pub enum Status { #[default] Valid = "valid", Invalid = "invalid", Warn = "warn", Accent = "accent", Todo = "todo" }
}

keyword! {
    /// Avatar `tone`.
    #[derive(Default)]
    pub enum AvatarTone { #[default] Ocean = "ocean", Accent = "accent", Cultivate = "cultivate", Muted = "muted" }
}

keyword! {
    /// Avatar `shape`.
    #[derive(Default)]
    pub enum AvatarShape { #[default] Square = "square", Circle = "circle" }
}

keyword! {
    /// EmptyState `variant`.
    #[derive(Default)]
    pub enum EmptyVariant { #[default] Block = "block", Inline = "inline" }
}

/// Spinner `tone`: a tone, or `inverse` (translucent white on a dark ground).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpinnerTone {
    Tone(Tone),
    Inverse,
}

impl Default for SpinnerTone {
    fn default() -> Self {
        Self::Tone(Tone::Accent)
    }
}

keyword! {
    /// Toast `tone`.
    #[derive(Default)]
    pub enum ToastTone { #[default] Success = "success", Info = "info", Warn = "warn", Error = "error" }
}

keyword! {
    /// OptionCard `control`.
    #[derive(Default)]
    pub enum OptionControl { #[default] None = "none", Radio = "radio" }
}

keyword! {
    /// AppHeader `tone`.
    #[derive(Default)]
    pub enum HeaderTone { #[default] Ocean = "ocean", Light = "light" }
}

keyword! {
    /// Callout `tone`.
    #[derive(Default)]
    pub enum CalloutTone { #[default] Info = "info", Warn = "warn", Kindle = "kindle", Success = "success" }
}

keyword! {
    /// Card `variant`.
    #[derive(Default)]
    pub enum CardVariant { #[default] Default = "default", Flat = "flat", Muted = "muted", Ocean = "ocean", Paper = "paper" }
}

keyword! {
    /// HelpCard `kind`.
    #[derive(Default)]
    pub enum HelpKind { #[default] Note = "note", Word = "word" }
}

keyword! {
    /// BookTile `layout`.
    #[derive(Default)]
    pub enum TileLayout { #[default] Tile = "tile", Row = "row" }
}

keyword! {
    /// VerseMarker `state`.
    #[derive(Default)]
    pub enum MarkerState { #[default] Idle = "idle", Dragging = "dragging" }
}

keyword! {
    /// WordChip `state`.
    #[derive(Default)]
    pub enum ChipState { #[default] Default = "default", Selected = "selected", Suggested = "suggested", Placed = "placed" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_round_trip_through_their_strings() {
        for t in Tone::ALL {
            assert_eq!(Tone::parse(t.as_str()), Some(*t));
        }
        for r in TextRole::ALL {
            assert_eq!(TextRole::parse(r.as_str()), Some(*r));
        }
        assert_eq!(Corner::parse("2xl"), Some(Corner::X2l));
        assert_eq!(Edge::parse("dashed-tone"), Some(Edge::DashedTone));
        assert_eq!(Tone::parse("primary"), None);
    }

    #[test]
    fn a_radius_is_a_keyword_or_a_css_length_never_a_typo() {
        assert_eq!(Radius::parse("2xl"), Some(Radius::Corner(Corner::X2l)));
        assert_eq!(Radius::parse("6px"), Some(Radius::Css("6px".into())));
        assert_eq!(
            Radius::parse("var(--radius-md)"),
            Some(Radius::Css("var(--radius-md)".into()))
        );
        assert_eq!(Radius::parse("0"), Some(Radius::Css("0".into())));
        assert_eq!(Radius::parse("large"), None);
        assert_eq!(
            Radius::parse("0px 4px"),
            Some(Radius::Css("0px 4px".into()))
        );
    }

    #[test]
    fn unknown_keywords_pass_through_as_css() {
        assert_eq!(
            Border::parse("2px solid red"),
            Border::Css("2px solid red".into())
        );
        assert_eq!(Border::parse("line"), Border::Edge(Edge::Line));
        assert_eq!(Pad::from("10px 12px"), Pad::Css("10px 12px".into()));
        assert_eq!(Pad::from("md"), Pad::Inset(Inset::Md));
        assert_eq!(Elevation::parse("hover"), Elevation::Shadow(Shadow::Hover));
    }
}
