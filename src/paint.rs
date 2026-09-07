//! COLOUR, AND ONLY WHERE SOMEBODY IS LOOKING.
//!
//! The usual reader of this program is an agent reading a pipe, and an
//! escape sequence is noise to it — worse than noise, since it lands in
//! the middle of the token it was meant to highlight. So the default is
//! `auto`: colour when standard output is a terminal and `NO_COLOR` is
//! unset, and nothing otherwise.
//!
//! `--json` never reaches here at all: the rendering gate calls the
//! human half only when JSON was not asked for.
//!
//! `color: never` exists for the terminal that shows the escapes rather
//! than obeying them — a console that never turned on its virtual
//! terminal processing — because the alternative is guessing at a
//! machine we cannot see.

use std::io::IsTerminal;
use std::sync::OnceLock;

/// Whether to paint at all. Asked once: it cannot change mid-run, and
/// this sits inside table loops.
pub fn wanted() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| match crate::config::color().0 {
        Some(on) => on,
        // NO_COLOR IS HONOURED BY ITS PRESENCE, whatever it holds — that
        // is what the convention says, and a tool that wanted a value
        // would be the one tool everybody has to special-case.
        None => std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal(),
    })
}

fn wrap(code: &str, text: &str) -> String {
    if wanted() { format!("\x1b[{code}m{text}\x1b[0m") } else { text.to_string() }
}

/// Said quietly: headings, and the paragraphs under a table that explain
/// what it means rather than what it says.
pub fn dim(text: &str) -> String {
    wrap("2", text)
}

/// THE NUMBER THE TOOL EXISTS FOR, coloured by what it is worth.
///
/// Not decoration: a table of nine rows where one is the one that
/// matters reads faster in colour than it ever will in alignment. The
/// scale is the same everywhere it appears so the eye can learn it once.
pub fn by_ratio(ratio: f64, text: &str) -> String {
    match ratio {
        r if r >= 0.5 => wrap("32", text),
        r if r >= 0.2 => wrap("33", text),
        _ => dim(text),
    }
}

/// Something that wants reading now — a mute job, a stranded ending.
pub fn alarm(text: &str) -> String {
    wrap("31", text)
}
