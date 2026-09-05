//! jbx — wrap every command line, and detach the ones that turn
//! out to be long.
//!
//! ────────────────────────────────────────────────────────────────────
//! WHY IT WRAPS EVERYTHING
//! ────────────────────────────────────────────────────────────────────
//!
//! Because what makes a line slow is not knowable before it runs. A list
//! of "commands worth wrapping" is a prediction, and that prediction was
//! measured on 136 real calls and refused: four of the five long shapes
//! had been seen exactly once, so no threshold recovered the time. By
//! the time a rule knows a command is slow you have already waited
//! through it, and it does not come back.
//!
//! So this one predicts nothing. It RUNS the line and finds out.
//!
//! ────────────────────────────────────────────────────────────────────
//! WHY THAT IS SAFE, AND IT IS MEASURED, NOT ASSUMED
//! ────────────────────────────────────────────────────────────────────
//!
//! The fear about wrapping everything is real: a wrapper that detaches
//! something waiting for a password produces a job nobody can ever
//! answer. Under an agent harness that cannot happen, and the reasons
//! were measured inside Claude Code's shell tool:
//!
//!   * no tty on any of the three streams — they are sockets and pipes,
//!     so capturing output loses nothing that was not already captured;
//!   * no controlling terminal at all: `/dev/tty` answers "No such
//!     device or address", so a command that wants to prompt fails in
//!     twelve milliseconds instead of hanging;
//!   * standard input already at end of file — `cat` returns empty
//!     immediately, `read` reads nothing.
//!
//! NOTHING CAN WAIT FOR A HUMAN THERE. And where something can — a real
//! terminal — this wrapper hands the line straight to a shell and stops
//! existing. The dangerous case is the one it refuses to be in.
//!
//! ────────────────────────────────────────────────────────────────────
//! WHAT IT NEVER DOES
//! ────────────────────────────────────────────────────────────────────
//!
//! It never loses an exit code: finished in time, the code is returned
//! unchanged; detached, it is written down where `status` and `wait`
//! read it. It never holds output back: the log is poured through as it
//! is written, flushed every time. And it never breaks a command to save
//! a token — every failure of rtk, of the store, of the supervisor ends
//! with the original line running.

/// Print a line, and let a closed pipe be what it is.
///
/// `jbx list | head` CLOSES THE PIPE ON PURPOSE. Rust ignores `SIGPIPE`
/// at startup, so the write comes back as an error and `outln!`
/// PANICS — a stack trace where every other Unix tool simply stops.
/// This writes and drops the error, which is the behaviour a pipeline
/// expects.
#[macro_export]
macro_rules! outln {
    () => { { use std::io::Write; let _ = writeln!(std::io::stdout().lock()); } };
    ($($arg:tt)*) => { {
        use std::io::Write;
        let _ = writeln!(std::io::stdout().lock(), $($arg)*);
    } };
}

pub mod config;
pub mod describe;
pub mod hook;
pub mod init;
pub mod input;
pub mod run;
pub mod signals;
pub mod slots;
pub mod stats;
pub mod store;

/// HOW LONG A LINE MAY HOLD THE CALLER before it is detached.
///
/// Thirty seconds, and the number is arbitrary in a way worth admitting:
/// what the measurement settled is that a threshold cannot be DERIVED
/// from history. It is a preference, so it is a setting.
pub fn default_after() -> f64 {
    config::after().0
}

/// Everything the tail of a command line means, in one place.
///
/// `--` ENDS THE OPTIONS AND STARTS THE LINE. Everything after it is the
/// command being wrapped, options included; it is joined back with
/// single spaces because a shell would have split it that way anyway,
/// and the hook passes it as ONE argument so that nothing is split at
/// all.
pub fn tail(args: &[String]) -> String {
    let rest = match args.iter().position(|a| a == "--") {
        Some(i) => &args[i + 1..],
        None => args,
    };
    rest.join(" ")
}
