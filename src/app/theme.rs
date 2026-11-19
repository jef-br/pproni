//! Console styling for the shell: palette, full-width rule, and the callout line closing every
//! screen (`⏵` marker, text, right-aligned version slug). Hand-tuned: no logic but the padding.
//!
//! Colours are 24-bit ANSI matching the design mockups' hex; `console::init` enables VT first.
//! Classic conhost rounds them to its nearest 16: acceptable, Windows Terminal is the target.

pub const ORANGE: &str = "\x1b[38;2;255;172;24m"; // #ffac18 — logo, ⏵ marker, finish link
pub const WHITE: &str = "\x1b[38;2;255;255;255m"; // #ffffff — status lines, callout text
pub const TEAL: &str = "\x1b[38;2;51;204;187m"; // #33ccbb — progress bar, start link
pub const GREY: &str = "\x1b[38;2;102;102;102m"; // #666666 — banner title and tagline
pub const DARK: &str = "\x1b[38;2;68;68;68m"; // #444444 — rules and the version slug
pub const MAGENTA: &str = "\x1b[38;2;255;85;255m"; // #ff55ff — the headcut question
pub const BOLD: &str = "\x1b[1m";
pub const RESET: &str = "\x1b[0m";

/// Width of every rule, and the column the slug is flush against: fits the longest localized
/// callout plus the slug, and does not wrap in a default 100-column console.
pub const WIDTH: usize = 97;

/// Vendor half of the version slug; the version itself comes from Cargo.
const VENDOR: &str = "PPTech";

/// Left margin shared by the rules, the progress bar and the folder lines.
pub const INDENT: usize = 2;

/// Visible prefix of a callout line: indent, marker, one space. Derived from `INDENT` so it can't
/// drift from what `callout` prints (a silent one-column overhang past the rule).
const MARKER_WIDTH: usize = INDENT + 2;

/// The separator around each block: runs to [`WIDTH`], opening with two blanks to line up with the
/// indented logo.
pub fn rule() -> String {
    format!("  {DARK}{}{RESET}", "─".repeat(WIDTH - INDENT))
}

/// The closing line of every screen: ` ⏵ ` marker and `text` left, version slug flush against
/// [`WIDTH`]. The gap saturates at one space: a `text` that hits the slug wraps the line (ugly),
/// where an underflow would panic in release.
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
