//! ONE LINE PER JOB EVENT, FOR SOMETHING THAT WATCHES.
//!
//! `jbx tail -f` streams one job's OUTPUT. This streams what happens to
//! jobs — let go of, ended, lost — which is what a watcher wants: not
//! what the build printed, but that the build finished and with what.
//!
//! IT OBSERVES AND DOES NOT CONSUME. `jbx signals` destroys what it
//! reports, which is right for an agent reading its own mail exactly
//! once and ruinous for a watcher: a monitor polling it would eat the
//! endings the session's own hook is waiting for. So this reads the
//! store and takes nothing.
//!
//! IT COVERS THE FAILURES, NOT ONLY THE HAPPY PATH. A watch that emits
//! only on success is silent through a crash, and silence looks exactly
//! like "still running". Every terminal state gets a line: an exit code
//! whatever it is, and `gone` for a job nothing recorded a code for.
//!
//! AND IT ENDS. A watch armed for ever after its event has fired is the
//! failure mode the harness warns about, so this exits when nothing is
//! running or queued any more — there is nothing left that could change.

use std::collections::BTreeMap;
use std::io::Write;

use crate::store::{self, State};

/// What we last said about a job, so that only CHANGES are events.
fn moment(state: &State) -> String {
    match state {
        State::Queued => "queued".into(),
        State::Running { detached: Some(true), .. } => "background".into(),
        State::Running { detached: Some(false), .. } => "foreground".into(),
        State::Running { .. } => "running".into(),
        State::Finished { code } => format!("finished {code}"),
        State::Lost => "gone".into(),
    }
}

pub fn watch(all: bool, as_json: bool) -> i32 {
    let me = crate::gain::project().1;
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    let mut first = true;
    loop {
        let records: Vec<store::Record> =
            store::all().into_iter().filter(|r| all || r.project == me).collect();
        let mut live = 0;
        for r in &records {
            // SETTLED, BECAUSE THIS IS THE VERB THAT ANNOUNCES ENDINGS.
            // A supervisor between its last write and its exit is
            // momentarily neither running nor recorded, and `state_of`
            // calls that `gone` — the state that means killed. `wait`
            // and `run` already ask twice before saying so; watch did
            // not, and a macOS runner announced a clean exit as a death.
            // The second look costs 250 ms and only on that answer.
            let state = store::settled_state(r);
            if matches!(state, State::Queued | State::Running { .. }) {
                live += 1;
            }
            let now = moment(&state);
            // THE FIRST PASS IS NOT A FLOOD OF EVENTS. Everything already
            // in the store would otherwise arrive as news, and a watcher
            // started after a busy afternoon would be told about it all.
            // What is already running is recorded and stays quiet until
            // it changes.
            if first {
                seen.insert(r.id.clone(), now);
                continue;
            }
            if seen.get(&r.id) == Some(&now) {
                continue;
            }
            seen.insert(r.id.clone(), now.clone());
            say(r, &state, &now, as_json);
        }
        first = false;
        // NOTHING LEFT THAT COULD CHANGE. Ending here is what keeps a
        // watcher from staying armed after the thing it waited for.
        if live == 0 {
            return 0;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

/// ONE LINE, FLUSHED. Anything reading this is reading a pipe, where a
/// line sitting in a buffer is a line that never happened.
fn say(r: &store::Record, state: &State, now: &str, as_json: bool) {
    let line = if as_json {
        // ONE OBJECT PER LINE, not an array: an array is only valid once
        // it is closed, and a stream is never closed until it is over.
        serde_json::json!({
            "id": r.id,
            "state": now,
            "exit": match state { State::Finished { code } => Some(code), _ => None },
            "intent": r.intent,
            "line": r.command,
            "project": r.project,
            "for_secs": store::now() - r.started,
        })
        .to_string()
    } else {
        format!("{:<10} {:<14} {}", r.id, now, r.intent)
    };
    let out = std::io::stdout();
    let mut out = out.lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}
