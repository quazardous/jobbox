//! IS IT STOPPED READING ITS STANDARD INPUT?
//!
//! THAT QUESTION IS NOT THE ONE WORTH ASKING, AND THIS FILE USED TO
//! PRETEND IT WAS. It answered "waiting for input" and the caller
//! announced that the line would never finish on its own — advice to
//! kill it, on commands that were about to succeed (#2063).
//!
//! Being stopped in `read` on file descriptor 0 says only that: the
//! process is reading its input. A pipeline stage whose producer is slow
//! looks exactly like a program waiting for a human, and so does a
//! `docker` client relaying a terminal it was handed. `sleep 5 | cat`
//! reproduces it in one line, and finishes with code 0.
//!
//! THE MEASUREMENT THAT MISSED IT was a witness chosen to agree: a `cat`
//! on a pty with nobody writing. It confirmed that a stuck process shows
//! `read(0)`. It never asked whether a process showing `read(0)` is
//! stuck — which is the direction the code actually needed.
//!
//! So this reports an OBSERVATION and nothing more. Whether it means
//! anything is decided by the caller, and only after it has lasted.

/// `read`, WHICH IS NUMBERED DIFFERENTLY ON EACH ARCHITECTURE.
///
/// An unknown machine disables the detector rather than reading some
/// other syscall's number and confidently naming the wrong thing.
#[cfg(target_os = "linux")]
const SYS_READ: Option<u64> = if cfg!(target_arch = "x86_64") {
    Some(0)
} else if cfg!(target_arch = "aarch64") {
    Some(63)
} else if cfg!(target_arch = "x86") {
    Some(3)
} else {
    None
};

/// The pid of a descendant of `root` currently stopped in a read of its
/// own file descriptor 0.
///
/// THE NAME IS THE WHOLE FIX. It used to be `waiting_on_input`, and a
/// name that answers a bigger question than the code does is how a
/// caller comes to assert something nobody measured.
///
/// THE WHOLE SUBTREE IS WALKED, not just the child we started: our child
/// is the shell, and it is whatever the shell ran that reads.
#[cfg(target_os = "linux")]
pub fn reading_its_input(root: u32) -> Option<u32> {
    let sys_read = SYS_READ?;
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        let children = std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"));
        let Ok(children) = children else { continue };
        stack.extend(children.split_whitespace().filter_map(|k| k.parse::<u32>().ok()));

        let Ok(line) = std::fs::read_to_string(format!("/proc/{pid}/syscall")) else { continue };
        let mut fields = line.split_whitespace();
        // "running" reads as `-1` here, and a task we may not look at
        // reads as nothing at all. Both simply do not answer.
        let (Some(nr), Some(arg0)) = (fields.next(), fields.next()) else { continue };
        let Ok(nr) = nr.parse::<u64>() else { continue };
        let fd = arg0.strip_prefix("0x").and_then(|h| u64::from_str_radix(h, 16).ok());
        if nr == sys_read && fd == Some(0) {
            return Some(pid);
        }
    }
    None
}

/// NO ANSWER RATHER THAN A GUESS.
///
/// Windows exposes no equivalent of `/proc/<pid>/syscall`, and the other
/// Unixes number their syscalls their own way. Saying "I do not know"
/// costs one message that is slightly less precise; guessing would cost
/// a command declared stuck while it was working.
#[cfg(not(target_os = "linux"))]
pub fn reading_its_input(_root: u32) -> Option<u32> {
    None
}
