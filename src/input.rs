//! TELLING A LONG COMMAND FROM A BLOCKED ONE.
//!
//! From outside they are the same thing: a process that has not
//! finished. They want opposite answers — detach the first, say so about
//! the second — because a detached prompt is a job that will never end.
//!
//! MEASURED, TWICE, AGAINST THREE WITNESSES: a `cat` reading a pty stops
//! in `read` on file descriptor 0; a `sleep` stops in `clock_nanosleep`;
//! a computing loop is in state `R` and is stopped in nothing at all.
//! The reading is `/proc/<pid>/syscall`, whose first two fields are the
//! syscall number and its first argument.

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

/// The pid of a descendant of `root` stopped reading its standard input.
///
/// THE WHOLE SUBTREE IS WALKED, not just the child we started: our child
/// is the shell, and what blocks is whatever the shell ran.
#[cfg(target_os = "linux")]
pub fn waiting_on_input(root: u32) -> Option<u32> {
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
pub fn waiting_on_input(_root: u32) -> Option<u32> {
    None
}
