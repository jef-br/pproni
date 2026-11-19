//! Startup banner: block logo with the name and tagline beside it, atop all three screens (the
//! status lines under it belong to each screen). Hand-tuned text and padding, no logic.
//! `LOGO`, `NAME` and `TAGLINE` are plain text, coloured in `print`: format braces in them print
//! literally. The version is in the callout slug (`theme::slug`), not here.

use super::theme::{BOLD, DARK, GREY, ORANGE, RESET};

/// Block logo, one row per line.
const LOGO: &[&str] = &[
    "",
    "  ██████╗ ██████╗                             ",
    "  ██   ██╗██   ██╗██████╗  █████╗ ███╗  ██╗██╗",
    "  ██████╔╝██████╔╝██   ██╗██   ██╗████╗ ██║██║",
    "  ██╔═══╝ ██╔═══╝ ██████╔╝██   ██║██╔██╗██║██║",
    "  ██║     ██║     ██║  ██║╚█████╔╝██║ ╚███║██║",
    "  ╚═╝     ╚═╝     ╚═╝  ╚═╝ ╚════╝ ╚═╝  ╚══╝╚═╝",
    "",
];

/// Product name, printed bold beside the logo.
const NAME: &str = "Post-Production retouche on normal images";

/// The three `→` lines under the name. Product shorthand, kept in English in every UI language.
const TAGLINE: &[&str] = &[
    "→ BridgeBatch but faster",
    "→ Center & Stretch + CropSquare + 'on white'",
    "→ Save as JPG in sRGB. 800-2000px",
];

/// Gap between the widest logo row and the text column beside it.
const HORIZONTAL_CHARSPACE_BETWEEN_LOGO_AND_TAGLINE: usize = 4;

/// Logo row the name sits on (the art's body, not its top edge); the tagline follows below it.
const LINEHEIGHT_OF_TAGLINE: usize = 2;

/// Print the banner: the logo block, with the name and tagline in the column beside it. `dim`
/// paints all of it in the slug's dark grey, for a screen that is waiting on a question.
pub fn print(dim: bool) {
    let (logo_colour, text_colour) = if dim { (DARK, DARK) } else { (ORANGE, GREY) };

    let width = LOGO.iter().map(|l| l.chars().count()).max().unwrap_or(0);

    let mut text = vec![format!("{BOLD}{text_colour}{NAME}{RESET}")];
    text.extend(TAGLINE.iter().map(|l| format!("{text_colour}{l}{RESET}")));

    
    for (i, row) in LOGO.iter().enumerate() {
        let pad = " ".repeat(width - row.chars().count() + HORIZONTAL_CHARSPACE_BETWEEN_LOGO_AND_TAGLINE);
        match i.checked_sub(LINEHEIGHT_OF_TAGLINE).and_then(|n| text.get(n)) {
            Some(line) => println!("{logo_colour}{row}{RESET}{pad}{line}"),
            None => println!("{logo_colour}{row}{RESET}"),
        }
    }
}
