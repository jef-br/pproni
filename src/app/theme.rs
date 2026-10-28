//! Console styling shared by the shell: the palette, the full-width rule, and the callout line
//! that closes every screen — `⏵` marker, text, and the right-aligned version slug.
//!
//! Hand-tuned on purpose — there is no logic here beyond the callout's padding. Colours are
//! 24-bit ANSI, matching the hex values in the design mockups; `console::init` turns VT processing
//! on before any of this is printed. Classic conhost quantises truecolour to its nearest 16, which
//! is an acceptable degradation — Windows Terminal is the target.

pub const ORANGE: &str = "\x1b[38;2;255;172;24m"; // #ffac18 — logo, ⏵ marker, finish link
pub const WHITE: &str = "\x1b[38;2;255;255;255m"; // #ffffff — status lines, callout text
pub const TEAL: &str = "\x1b[38;2;51;204;187m"; // #33ccbb — progress bar, start link
pub const GREY: &str = "\x1b[38;2;102;102;102m"; // #666666 — banner title and tagline
pub const DARK: &str = "\x1b[38;2;68;68;68m"; // #444444 — rules and the version slug
pub const MAGENTA: &str = "\x1b[38;2;255;85;255m"; // #ff55ff — the headcut question
pub const BOLD: &str = "\x1b[1m";
pub const RESET: &str = "\x1b[0m";

/// Width of every horizontal rule, and the column the version slug is flush against. Wide enough
/// for the longest localized callout line plus the slug, narrow enough not to wrap in a default
/// 100-column console.
pub const WIDTH: usize = 97;

/// Vendor half of the version slug; the version itself comes from Cargo.
const VENDOR: &str = "PPTech";

/// Left margin shared by the rules, the progress bar and the folder lines.
pub const INDENT: usize = 2;

/// Visible prefix of a callout line: the shared indent, the marker, one space. Derived from
/// `INDENT` rather than written out, so moving the marker can't desync it from what `callout`
/// actually prints — that is a silent one-column overhang past the rule.
const MARKER_WIDTH: usize = INDENT + 2;

/// The separator that brackets each block. It runs to [`WIDTH`] like everything else on screen,
/// but opens with two blanks instead of two dashes, so it lines up with the indented logo.
pub fn rule() -> String {
    format!("  {DARK}{}{RESET}", "─".repeat(WIDTH - INDENT))
}

/// The closing line of every screen: the ` ⏵ ` marker and `text` on the left, the version slug
/// flush against [`WIDTH`] on the right.
///
/// The gap saturates at one space. A localized `text` long enough to collide with the slug pushes
/// the line past `WIDTH` and wraps — ugly, but an underflow here would panic in release.
pub fn callout(text: &str) -> String {
    let slug = slug();
    let used = MARKER_WIDTH + text.chars().count() + slug.chars().count();
    let gap = WIDTH.saturating_sub(used).max(1);
    format!(
        "{ORANGE}{}⏵ {RESET}{WHITE}{text}{RESET}{}{DARK}{slug}{RESET}",
        " ".repeat(INDENT),
        " ".repeat(gap)
    )
}

/// A callout that asks a question: the marker and `text` in [`MAGENTA`], the slug as usual.
pub fn question_callout(text: &str) -> String {
    let slug = slug();
    let used = MARKER_WIDTH + text.chars().count() + slug.chars().count();
    let gap = WIDTH.saturating_sub(used).max(1);
    format!(
        "{MAGENTA}{}⏵ {BOLD}{text}{RESET}{}{DARK}{slug}{RESET}",
        " ".repeat(INDENT),
        " ".repeat(gap)
    )
}

/// The 1-based column one space after `text` on a callout line — where the answer is typed.
pub fn answer_column(text: &str) -> usize {
    MARKER_WIDTH + text.chars().count() + 2
}

/// `V1.0.0 – PPTech`, the slug sitting at the right end of every callout line.
pub fn slug() -> String {
    format!("V{} - {VENDOR}", env!("CARGO_PKG_VERSION"))
}
