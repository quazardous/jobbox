//! WHAT THE WRAPPING COSTS, MEASURED RATHER THAN CLAIMED.
//!
//! `jbx gain` says what the detaching bought. Saying that without ever
//! saying what it costs is half a sentence, and the half that flatters
//! us. This is the other half.
//!
//! THREE NUMBERS, AND THE FIRST IS THE ONE THAT MATTERS.
//!
//! 1. THE HOOK, which runs on every command an agent issues — including
//!    the overwhelming majority that finish in under a second and are
//!    never touched again. Nobody escapes this one, so it is the figure
//!    to be honest about.
//! 2. THE WRAPPER on a line that finishes before the cut: a supervisor
//!    spawned, a log opened, a few polls, and a record removed again.
//! 3. Their sum, which is what a short command actually pays.
//!
//! WHAT IS NOT MEASURED, ON PURPOSE. The detached path is not timed
//! here. A command that outlasts thirty seconds is dominated by itself
//! by four orders of magnitude, and quoting an overhead percentage
//! against it would be arithmetic designed to look good.
//!
//! AND THE TWO SIDES ARE INTERLEAVED, never all of one then all of the
//! other. A machine's mood drifts — a CPU boosts, a cache fills, another
//! process wakes — and a run of A followed by a run of B measures that
//! drift as faithfully as it measures the difference.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The payload a client really sends, so the hook does its real work.
const PAYLOAD: &str = r#"{"hook_event_name":"PreToolUse","session_id":"jbx-bench","tool_name":"Bash","tool_input":{"command":"echo hello","description":"a short command"}}"#;

/// The middle value, which is what a per-command cost should be read as:
/// a mean is moved by one scheduling hiccup, and there is always one.
fn median(mut xs: Vec<Duration>) -> Duration {
    xs.sort();
    xs[xs.len() / 2]
}

/// And the tail, because a wrapper that is usually free and occasionally
/// slow is not free.
fn p90(mut xs: Vec<Duration>) -> Duration {
    xs.sort();
    xs[(xs.len() * 9) / 10 - usize::from(xs.len() >= 10)]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Everything a child needs to behave as it would under an agent.
fn child(program: &std::path::Path) -> Command {
    let mut cmd = Command::new(program);
    // WITHOUT THIS THE MEASUREMENT IS OF NOTHING. `JBX_WRAPPED` tells an
    // inner jbx that somebody outside already holds this job, so it
    // steps aside and runs the line directly — which is correct
    // behaviour and would time the shell instead of the wrapper.
    cmd.env_remove("JBX_WRAPPED");
    cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd
}

fn once(mut cmd: Command, feed: Option<&str>) -> Duration {
    let at = Instant::now();
    let Ok(mut ch) = cmd.spawn() else { return Duration::ZERO };
    if let Some(text) = feed {
        if let Some(mut into) = ch.stdin.take() {
            let _ = into.write_all(text.as_bytes());
        }
    }
    drop(ch.stdin.take());
    let _ = ch.wait();
    at.elapsed()
}

/// `jbx bench` — WHAT THE WRAPPING COSTS, per command.
pub fn bench(runs: usize, json: bool) -> i32 {
    let runs = runs.clamp(5, 2000);
    let me = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("jbx: cannot find my own binary: {e}");
            return 2;
        }
    };
    let shell = std::path::PathBuf::from("/bin/sh");
    if !shell.exists() {
        eprintln!("jbx: this needs /bin/sh to have something to compare against");
        return 2;
    }

    let (mut hook, mut raw, mut wrapped) = (vec![], vec![], vec![]);
    // A WARM-UP THAT IS THROWN AWAY. The first spawn of a binary pays
    // for reading it off disk, and quoting that as the per-command cost
    // would be measuring the page cache.
    for _ in 0..3 {
        once(child(&me), None);
        once(child(&shell), None);
    }

    for _ in 0..runs {
        // Interleaved, one of each, in the same conditions.
        let mut h = child(&me);
        h.arg("hook").arg("claude");
        hook.push(once(h, Some(PAYLOAD)));

        let mut r = Command::new(&shell);
        r.arg("-c").arg("true");
        r.env_remove("JBX_WRAPPED");
        r.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        raw.push(once(r, None));

        let mut w = child(&me);
        w.arg("run").arg("--").arg("true");
        wrapped.push(once(w, None));
    }

    let (hm, rm, wm) = (median(hook.clone()), median(raw), median(wrapped.clone()));
    // Computed once: the tail of the wrapped side, minus the shell the
    // line would have cost anyway.
    let wrapper_p90 = p90(wrapped).saturating_sub(rm);
    // The wrapper's own cost is what it adds to running the line, so the
    // shell it would have taken anyway comes off.
    let added = wm.saturating_sub(rm);
    let total = hm + added;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "runs": runs,
                "hook_ms": ms(hm),
                "hook_p90_ms": ms(p90(hook)),
                "wrapper_ms": ms(added),
                "wrapper_p90_ms": ms(wrapper_p90),
                "per_command_ms": ms(total),
                "bare_shell_ms": ms(rm),
            })
        );
        return 0;
    }

    println!("  what the wrapping costs, over {runs} interleaved runs\n");
    println!("  {:<34}{:>8.1} ms", "the hook, on every command", ms(hm));
    println!("  {:<34}{:>8.1} ms", "the wrapper, on a short line", ms(added));
    println!("  {:<34}{:>8.1} ms   \x1b[2m← nine runs in ten\x1b[0m", "  and at its slowest", ms(wrapper_p90));
    println!("  {:<34}{:>8.1} ms", "so a short command pays", ms(total));
    println!("\n  \x1b[2mfor comparison, a bare `/bin/sh -c true` takes {:.1} ms\x1b[0m", ms(rm));
    println!(
        "  \x1b[2mthe detached path is not timed: a command that outlasts the cut\n  \
         is thirty seconds long, and any percentage against that flatters us.\x1b[0m"
    );
    0
}
