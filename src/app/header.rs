//! Startup banner: the block logo with the product name and tagline beside it. Drawn at the top
//! of all three screens; the two status lines under it belong to the screen, not to the banner.
//!
//! Hand-tuned on purpose — there is no logic here, only text and padding. `LOGO`, `NAME` and
//! `TAGLINE` are plain text: the colours are applied in `print`, so keep format braces out of them
//! or they will be printed literally. The version is not shown here — it lives in the slug at the
//! right end of every callout line (`theme::slug`).

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

/// Logo row the name sits on, so it lines up with the body of the art rather than its top edge.
/// The tagline follows on the rows below.
const LINEHEIGHT_OF_TAGLINE: usize = 2;

/// Print the banner: the logo block, with the name and tagline in the column beside it. `dim`
/// paints all of it in the slug's dark grey, for a screen that is waiting on a question.
pub fn print(dim: bool) {
    let (logo_colour, text_colour) = if dim { (DARK, DARK) } else { (ORANGE, GREY) };

    // meassure logo width
    let width = LOGO.iter().map(|l| l.chars().count()).max().unwrap_or(0);

    //meassure text width
    let mut text = vec![format!("{BOLD}{text_colour}{NAME}{RESET}")];
    text.extend(TAGLINE.iter().map(|l| format!("{text_colour}{l}{RESET}")));

    
    for (i, row) in LOGO.iter().enumerate() {
        // 
        let pad = " ".repeat(width - row.chars().count() + HORIZONTAL_CHARSPACE_BETWEEN_LOGO_AND_TAGLINE);
        match i.checked_sub(LINEHEIGHT_OF_TAGLINE).and_then(|n| text.get(n)) {
            Some(line) => println!("{logo_colour}{row}{RESET}{pad}{line}"),
            None => println!("{logo_colour}{row}{RESET}"),
        }
    }
}
