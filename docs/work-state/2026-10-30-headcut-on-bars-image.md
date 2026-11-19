# Decided 2026-10-30: a headcut on a bars image leaves the bars out (user; code in `core::process_file`)

[Up: table of contents](TOC.md)

* Bug: on 46 (white bars top and bottom) the headcut cut at the philtrum, `peel` threw the trims away, so
  the bars rule never saw the bars and R1 stretched them. Wanted: white bars, nothing more.
* Rule: when a headcut is planned and the framing check says bars (`Decision::Centre`), the picture to cut
  is trim 1's box from the philtrum down (`headcut_in_bars`). The headcut is done only if the room below
  the cut is at least 75% of the mask's width (`HEADCUT_BARS_MAX_SHORTFALL` = 25%, the user's gut value;
  50% was too loose). If not: no headcut, and the bars rule centres the whole picture on white bars.
  No bars: the headcut is as before.
* Checked on the 62-image set: only 46 changed (headcut skipped: 349 px of room, 590 needed). Pass case
  only tried on made-up pictures (46 stretched taller): the headcut runs, bars stay out; the three touched
  edges then send it to R3, by the [2026-10-29 rule](2026-10-29-routing-in-shot-classifier.md).
* Open: only 46 tests the 25%; it wants more bars-with-a-face examples to calibrate.
