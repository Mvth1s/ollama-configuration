//! Direct port of `stream_ansi_aware` from `gui/src-tauri/src/main.rs`,
//! unchanged in behavior - only the imports and the removal of Tauri event
//! emission (that stays in `gui/src-tauri`, this crate has no `AppHandle` to
//! emit to) differ from the original. Kept here, rather than left
//! Tauri-only, because Phase 2's empirical investigation (pkexec/package
//! manager output capture) confirmed this exact parsing already correctly
//! handles the two best-behaved cases (`curl`'s `\r` progress bar, `ollama
//! pull`'s `ESC[1G` redraws) and needs no changes for this phase.
//!
//! `03-pull-models.sh`'s `ollama pull` renders its progress with ANSI
//! cursor-control sequences (`ESC [ 1 G` to return to column 1) rather than
//! ever printing `\n` mid-download; plain line-based reading
//! (`BufRead::lines()`, which only splits on `\n`) would buffer an entire
//! download silently. This treats a bare `\r` and `ESC [ <n> G` as line
//! boundaries too, so live progress can be streamed to a UI the way it
//! would render in a real terminal; other recognized CSI sequences (cursor
//! show/hide, erase-line, ...) are dropped rather than shown as literal
//! escape codes; anything not starting a CSI sequence is passed through
//! unchanged.

use std::io::{BufReader, Read};

enum State {
    Normal,
    Esc,
    Csi,
}

/// Reads `reader` byte-by-byte, calling `on_line` with each complete line as
/// soon as a `\n`, a bare `\r`, or an `ESC [ <n> G` (cursor-to-column)
/// sequence is seen, and once more for any trailing unterminated content at
/// EOF. Other CSI sequences are recognized and dropped without flushing a
/// line; a lone ESC not followed by `[` is passed through as a literal byte.
pub fn stream_ansi_aware<R: Read>(reader: R, mut on_line: impl FnMut(String)) {
    let mut state = State::Normal;
    let mut line: Vec<u8> = Vec::new();
    let mut reader = BufReader::new(reader);
    let mut byte = [0u8; 1];

    loop {
        match reader.read(&mut byte) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let b = byte[0];

        match state {
            State::Normal => match b {
                b'\n' | b'\r' => {
                    on_line(String::from_utf8_lossy(&line).into_owned());
                    line.clear();
                }
                0x1B => state = State::Esc,
                _ => line.push(b),
            },
            State::Esc => {
                if b == b'[' {
                    state = State::Csi;
                } else {
                    // Not a CSI sequence after all: re-dispatch this byte
                    // as if it had been read in Normal state, rather than
                    // silently dropping it.
                    state = State::Normal;
                    match b {
                        b'\n' | b'\r' => {
                            on_line(String::from_utf8_lossy(&line).into_owned());
                            line.clear();
                        }
                        0x1B => state = State::Esc,
                        _ => line.push(b),
                    }
                }
            }
            State::Csi => {
                // Final byte of a CSI sequence: any of 0x40..=0x7E: RFC-ish
                // ANSI/ECMA-48 convention, params are digits/';'/'?' before it.
                if (0x40..=0x7E).contains(&b) {
                    if b == b'G' {
                        on_line(String::from_utf8_lossy(&line).into_owned());
                        line.clear();
                    }
                    state = State::Normal;
                }
            }
        }
    }

    if !line.is_empty() {
        on_line(String::from_utf8_lossy(&line).into_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect_lines(input: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        stream_ansi_aware(std::io::Cursor::new(input), |line| lines.push(line));
        lines
    }

    #[test]
    fn passes_plain_newline_delimited_text_through_unchanged() {
        assert_eq!(collect_lines(b"line one\nline two\n"), vec!["line one", "line two"]);
    }

    #[test]
    fn flushes_a_trailing_line_with_no_terminator_at_eof() {
        assert_eq!(collect_lines(b"line one\nno newline at the end"), vec!["line one", "no newline at the end"]);
    }

    #[test]
    fn treats_bare_carriage_return_as_a_line_boundary() {
        assert_eq!(collect_lines(b"downloading 10%\rdownloading 20%\r"), vec!["downloading 10%", "downloading 20%"]);
    }

    #[test]
    fn treats_cursor_to_column_1_as_a_line_boundary() {
        // The exact pattern `ollama pull` was captured emitting for every
        // progress redraw: ESC [ 1 G to return to column 1.
        let input = b"downloading 10%\x1b[1Gdownloading 20%\x1b[1G";
        assert_eq!(collect_lines(input), vec!["downloading 10%", "downloading 20%"]);
    }

    #[test]
    fn drops_other_csi_sequences_without_flushing_a_line() {
        // ESC [ K (erase to end of line) and ESC [ ? 25 l (hide cursor), as
        // seen surrounding ollama's real progress redraws, should vanish
        // from the output rather than showing up as literal escape codes
        // or splitting a single line into extra empty ones.
        let input = b"pulling manifest\x1b[K\x1b[?25l done\n";
        assert_eq!(collect_lines(input), vec!["pulling manifest done"]);
    }

    #[test]
    fn preserves_a_byte_following_a_lone_escape_not_starting_a_csi_sequence() {
        let input = b"before\x1bxafter\n";
        assert_eq!(collect_lines(input), vec!["beforexafter"]);
    }

    #[test]
    fn curl_style_dotted_progress_bar_redraws_via_bare_carriage_return() {
        // Confirmed in Phase 2's empirical capture: curl's own progress
        // meter still uses `\r` even without a tty attached (unlike
        // pacman/dnf/zypper's network phase, which stay silent instead).
        let input = b"#####    38.7%\r######   40.0%\r######## 100.0%\n";
        assert_eq!(
            collect_lines(input),
            vec!["#####    38.7%", "######   40.0%", "######## 100.0%"]
        );
    }
}
