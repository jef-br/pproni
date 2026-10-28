//! Localized user-facing strings. English is the default; ES/FR/NL/IT/DE are supported.
//!
//! Every line here is printed inside the layout `app::theme` draws, so keep each one under
//! `theme::WIDTH` characters — a longer line wraps and pushes the rule below it out of place.
//! `interrupt_hint` and `close_line` are tighter still: they share their line with the version
//! slug on the right, so they have about `WIDTH - 18` to work with.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Es,
    Fr,
    Nl,
    It,
    De,
}

impl Lang {
    /// Map an ISO 639-1 primary language code to a supported language (English otherwise).
    pub fn from_code(code: &str) -> Lang {
        match code.to_ascii_lowercase().as_str() {
            "es" => Lang::Es,
            "fr" => Lang::Fr,
            "nl" => Lang::Nl,
            "it" => Lang::It,
            "de" => Lang::De,
            _ => Lang::En,
        }
    }
}

/// The two status lines of the start screen. The input folder is printed separately, below them.
pub fn drop_prompt(lang: Lang) -> [&'static str; 2] {
    match lang {
        Lang::En => [
            "  Put your images into the folder below.",
            "  Then double-click PProni again to start processing.",
        ],
        Lang::Es => [
            "  Coloca tus imágenes en la carpeta de abajo.",
            "  Luego vuelve a hacer doble clic en PProni para empezar.",
        ],
        Lang::Fr => [
            "  Placez vos images dans le dossier ci-dessous.",
            "  Double-cliquez ensuite sur PProni pour lancer le traitement.",
        ],
        Lang::Nl => [
            "  Zet je afbeeldingen in de map hieronder.",
            "  Dubbelklik daarna op PProni om de verwerking te starten.",
        ],
        Lang::It => [
            "  Inserisci le immagini nella cartella qui sotto.",
            "  Poi fai di nuovo doppio clic su PProni per iniziare.",
        ],
        Lang::De => [
            "  Lege deine Bilder in den Ordner unten.",
            "  Doppelklicke danach erneut auf PProni, um zu starten.",
        ],
    }
}

/// The question asked before a batch starts: headcutting on or off for every image. It takes the
/// place of a callout's text. The answer key is `Y` in every language, so the `(Y/N)` stays as
/// written.
pub fn headcuts_question(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Enable headcutting for ALL images? (Y/N)",
        Lang::Es => "¿Recortar cabezas para TODAS las imágenes? (Y/N)",
        Lang::Fr => "Découper les têtes de TOUTES les images ? (Y/N)",
        Lang::Nl => "Hoofden croppen op ALLE afbeeldingen? (Y/N)",
        Lang::It => "Attivare il TaglioTeste per TUTTE le immagini? (Y/N)",
        Lang::De => "KopfKnipf für ALLE Bilder aktivieren? (Y/N)",
    }
}

/// Callout of the start and finish screens.
pub fn close_line(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Press any key to close this window.",
        Lang::Es => "Presiona cualquier tecla para cerrar esta ventana.",
        Lang::Fr => "Appuyez sur une touche pour fermer cette fenêtre.",
        Lang::Nl => "Druk op een toets om dit venster te sluiten.",
        Lang::It => "Premi un tasto per chiudere questa finestra.",
        Lang::De => "Drücke eine beliebige Taste, um dieses Fenster zu schließen.",
    }
}

/// Callout shown under the progress bar while the batch runs.
pub fn interrupt_hint(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Close this window to stop processing",
        Lang::Es => "Cierra esta ventana para detener el procesamiento",
        Lang::Fr => "Fermez cette fenêtre pour arrêter le traitement",
        Lang::Nl => "Sluit dit venster om het verwerken te stoppen",
        Lang::It => "Chiudi questa finestra per interrompere l'elaborazione",
        Lang::De => "Schließe dieses Fenster, um die Verarbeitung zu stoppen",
    }
}

/// Label in front of the backup folder on the processing screen. Padded to match [`output_label`]
/// so the two paths line up.
pub fn backup_label(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "  Backup: ",
        Lang::Es => "  Copia:  ",
        Lang::Fr => "  Copie:  ",
        Lang::Nl => "  Backup: ",
        Lang::It => "  Backup: ",
        Lang::De => "  Backup: ",
    }
}

/// Label in front of the output folder on the processing screen. Padded to match
/// [`backup_label`].
pub fn output_label(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "  Output: ",
        Lang::Es => "  Output: ",
        Lang::Fr => "  Output: ",
        Lang::Nl => "  Output:",
        Lang::It => "  Output: ",
        Lang::De => "  Output:",
    }
}

/// Trailing word of the progress bar's `08m25s remaining` readout.
pub fn remaining(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "remaining",
        Lang::Es => "restante",
        Lang::Fr => "restant",
        Lang::Nl => "resterend",
        Lang::It => "rimanente",
        Lang::De => "verbleibend",
    }
}

/// The two status lines of the finish screen. `seconds` is already rounded to hundredths.
pub fn finished(lang: Lang, seconds: f64, ok: usize, failed: usize) -> [String; 2] {
    let s = format!("{seconds:.2}");
    match lang {
        Lang::En => [
            format!("  Batch finished in {s}s."),
            format!("  {ok} processed, {failed} skipped ... Your results are in the folder below:"),
        ],
        Lang::Es => [
            format!("  Lote terminado en {s}s."),
            format!("  {ok} procesadas, {failed} omitidas ... Tus resultados están en la carpeta de abajo:"),
        ],
        Lang::Fr => [
            format!("  Traitement terminé en {s}s."),
            format!("  {ok} traitées, {failed} ignorées ... Vos résultats sont dans le dossier ci-dessous:"),
        ],
        Lang::Nl => [
            format!("  Batch klaar in {s}s."),
            format!("  {ok} verwerkt, {failed} overgeslagen ... Je resultaten staan in de map hieronder:"),
        ],
        Lang::It => [
            format!("  Lotto completato in {s}s."),
            format!("  {ok} elaborate, {failed} saltate ... I risultati sono nella cartella qui sotto:"),
        ],
        Lang::De => [
            format!("  Stapel in {s}s fertig."),
            format!("  {ok} verarbeitet, {failed} übersprungen ... Deine Ergebnisse sind im Ordner unten:"),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::theme;

    const LANGS: [Lang; 6] = [Lang::En, Lang::Es, Lang::Fr, Lang::Nl, Lang::It, Lang::De];

    /// Status lines are printed flush left and must not wrap, or they push the rule below them out of place.
    #[test]
    fn status_lines_fit_the_frame() {
        for lang in LANGS {
            for line in drop_prompt(lang) {
                assert!(line.chars().count() <= theme::WIDTH, "{line:?}");
            }
            // 999 files and a 9999.99s run: the widest the counters realistically get.
            for line in finished(lang, 9999.99, 999, 999) {
                assert!(line.chars().count() <= theme::WIDTH, "{line:?}");
            }
        }
    }

    /// Callout lines share their row with the version slug on the right, so they have less room
    /// than a status line. A collision would push the slug past the rule and wrap it.
    #[test]
    fn callouts_leave_room_for_the_slug() {
        let budget = theme::WIDTH - 3 - theme::slug().chars().count() - 1;
        for lang in LANGS {
            for text in [close_line(lang), interrupt_hint(lang), headcuts_question(lang)] {
                assert!(text.chars().count() <= budget, "{text:?} exceeds {budget}");
            }
        }
    }
}
