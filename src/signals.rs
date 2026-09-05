//! ENDINGS, AND TELLING WHOEVER NEEDS TO KNOW.
//!
//! A detached job that nobody is told about is a job you have to
//! REMEMBER to check — and remembering is exactly what letting go of it
//! was supposed to buy. So an ending is deposited when it happens, and
//! read on the next turn by whoever was waiting for it.
//!
//! ────────────────────────────────────────────────────────────────────
//! ONE MAILBOX PER READER, AND ONE SHARED
//! ────────────────────────────────────────────────────────────────────
//!
//! Endings are READ AND ERASED in a single gesture, which is what makes
//! each one announced exactly once. With a single mailbox that same
//! property means the first reader to look blinds every other — so the
//! agent's mailbox is per session.
//!
//! The person's is shared ON PURPOSE: one human wants every ending,
//! whichever session started it.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde_json::Value;

use crate::{stats, store};

/// Who is told: the model, and the person.
pub const AUDIENCES: [&str; 2] = ["agent", "user"];

/// The person's mailbox is shared across sessions; the agent's is not.
const SHARED: [&str; 1] = ["user"];

fn signals_dir() -> PathBuf {
    store::dir().join("signals")
}

pub fn mailbox(client: &str, audience: &str) -> PathBuf {
    if SHARED.contains(&audience) {
        signals_dir().join(format!("{audience}.jsonl"))
    } else {
        signals_dir().join(client).join(format!("{audience}.jsonl"))
    }
}

/// WHO IS ASKING — one store for everyone, one mailbox each.
///
/// THE SESSION IS THE ADDRESS, and it has to be, because the two ends of
/// this must agree without talking to each other. An ending is deposited
/// by a supervisor that inherited the COMMAND's working directory; it is
/// read by a hook that runs in the SESSION's. Deriving the address from
/// the project made those two disagree the moment a session touched a
/// second repository — measured on a real store: one session held both a
/// `bms-…` and a `BookShepherd-…` mailbox, and two endings sat in the
/// one it had stopped reading.
///
/// The Python this replaces guarded against exactly this by writing the
/// project into a settings file and never deriving it. Removing that
/// guard, I checked that a repository root does not move when you `cd`
/// into a SUBDIRECTORY — and never asked what happens when you `cd` into
/// another project. A witness chosen too narrowly, again.
///
/// `CLAUDE_PROJECT_DIR` would have given the session's own project, and
/// it is not exposed to commands — only to hooks. So the session id it
/// is: both ends read the same variable, whatever directory they are in.
///
/// The project has not gone anywhere: every job record carries its own,
/// and `list` and `status` show it. It is a LABEL on the work, which it
/// always was. It was never an address.
pub fn client() -> String {
    if let Ok(pinned) = std::env::var("JBX_CLIENT") {
        if !pinned.is_empty() && plain(&pinned) {
            return pinned;
        }
    }
    let session: String = std::env::var("CLAUDE_CODE_SESSION_ID")
        .unwrap_or_default()
        .chars()
        .take(8)
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if !session.is_empty() {
        return format!("cc-{session}");
    }
    // NO SESSION: a plain shell. Two terminals in one project then share
    // a mailbox, which is right — the person wants every ending, and the
    // shared box already works that way.
    let (project, _) = stats::project();
    let project: String = project
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "._-".contains(*c))
        .collect();
    if project.is_empty() { "default".into() } else { project }
}

fn plain(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        && name.len() <= 64
}

/// NOTE THAT A JOB ENDED, for every audience.
///
/// Called by the supervisor, behind the back of somebody who has gone to
/// do something else. IT NEVER FAILS OUT LOUD: failing here would soil a
/// job that itself went fine.
pub fn deposit(id: &str, code: i32, intent: &str, log: &str, client: &str) {
    let line = serde_json::json!({
        "id": id,
        "code": code,
        "intent": intent,
        "log": log,
        "client": client,
        "finished_at": store::now(),
    })
    .to_string();
    for audience in AUDIENCES {
        let box_path = mailbox(client, audience);
        if let Some(parent) = box_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(mut file) = fs::OpenOptions::new().append(true).create(true).open(&box_path) {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// EMPTY ONE MAILBOX — read and erase, in a single gesture.
///
/// IT CLAIMS BY RENAMING. Reading and then deleting leaves a window: an
/// ending deposited between the two is erased before anyone saw it, and
/// a lost ending is silent by nature — there is nothing left to look at.
/// `rename` is atomic on both platforms; after it, a later ending opens
/// the path afresh and lands in a file the deletion never touches.
///
/// IT NEVER FAILS OUT LOUD: it is called from a hook, on every turn.
pub fn take(client: &str, audience: &str) -> Vec<Value> {
    let path = mailbox(client, audience);
    if !path.exists() {
        return Vec::new();
    }
    let claimed = path.with_extension(format!("jsonl.taken-{}", std::process::id()));
    if fs::rename(&path, &claimed).is_err() {
        return Vec::new(); // gone, or somebody claimed it first
    }
    let raw = fs::read_to_string(&claimed).unwrap_or_default();
    let _ = fs::remove_file(&claimed);
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        // A TRUNCATED LINE COSTS ONLY ITSELF. The neighbouring jobs did
        // finish, and their endings are what we came for.
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// What waits in ONE client's own mailboxes.
///
/// THE SHARED BOX IS NOT COUNTED HERE, and leaving it in was a real
/// mistake: `mailbox()` answers the same path for every client when the
/// audience is shared, so the person's endings were counted once per
/// session — three sessions turned two endings into six. It is counted
/// once, on its own line, where it belongs.
fn held_by(who: &str) -> usize {
    AUDIENCES
        .iter()
        .filter(|a| !SHARED.contains(a))
        .map(|a| lines_in(&mailbox(who, a)))
        .sum()
}

fn lines_in(path: &std::path::Path) -> usize {
    fs::read_to_string(path)
        .map(|t| t.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0)
}

/// What waits in the person's shared mailbox, whoever started it.
pub fn held_for_the_person() -> usize {
    SHARED.iter().map(|a| lines_in(&mailbox("", a))).sum()
}

/// Endings sitting in mailboxes that are not ours, busiest first.
///
/// WHOSE PROBLEM THIS IS. Naming clients from the session means a
/// session that goes away leaves its endings behind, addressed to
/// nobody. They are not lost — but only somebody looking here will ever
/// know they exist.
pub fn stranded(me: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(signals_dir()) else { return out };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let who = entry.file_name().to_string_lossy().into_owned();
        if who == me {
            continue;
        }
        let held = held_by(&who);
        if held > 0 {
            out.push((who, held));
        }
    }
    out.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    out
}

/// Every mailbox, with what waits in it — ours included.
pub fn all_clients() -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(signals_dir()) else { return out };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let who = entry.file_name().to_string_lossy().into_owned();
        let held = held_by(&who);
        out.push((who, held));
    }
    out.sort();
    out
}

fn ok(s: &Value) -> bool {
    s["code"].as_i64() == Some(0)
}

fn name(s: &Value) -> String {
    let intent = s["intent"].as_str().unwrap_or("");
    if intent.is_empty() {
        format!("job {}", s["id"].as_str().unwrap_or("?"))
    } else {
        intent.to_string()
    }
}

/// `jbx signals <audience>` — WHAT HAS FINISHED SINCE LAST TIME.
///
/// THIS VERB KNOWS NO HARNESS AND DOES NOT WANT TO. It returns facts;
/// the shaping — a hook's JSON, a desktop notification, a message — is
/// whoever integrates it.
pub fn signals(audience: &str, as_json: bool, who: Option<&str>) -> i32 {
    if !AUDIENCES.contains(&audience) {
        eprintln!("jbx: audience must be one of {}", AUDIENCES.join(", "));
        return 2;
    }
    let who = who.map(String::from).unwrap_or_else(client);
    for s in take(&who, audience) {
        if as_json {
            outln!("{s}");
            continue;
        }
        let state = if ok(&s) {
            "OK".to_string()
        } else {
            format!("FAILED (exit={})", s["code"].as_i64().unwrap_or(-1))
        };
        outln!(
            "  job {}  {}  {}  — {}",
            s["id"].as_str().unwrap_or("?"),
            name(&s),
            state,
            s["log"].as_str().unwrap_or("—")
        );
    }
    0
}

/// SAID ONCE, WHEN A SESSION OPENS.
///
/// THE DISCIPLINE IS CARRIED BY THE TOOL, NOT BY A DOCUMENT. A skill
/// describing when to background something was the old answer, and it
/// asked the reader to make a judgement they get wrong — which command
/// will be long. This says the only thing left to decide: whether the
/// answer is needed BEFORE anything else can happen.
///
/// On a session's first hook and nowhere else. A rule repeated every
/// turn is a banner, and a banner is not read.
pub fn discipline() -> i32 {
    outln!("[jbx] Every command here is wrapped: if it turns out to be long it detaches");
    outln!("      itself and hands the shell back, and you are told when it ends. So do");
    outln!("      not background things by hand, and do not sit on a build in case.");
    outln!("      Before running something, ask whether you need its result BEFORE you");
    outln!("      can do anything else. If you do, say so: `jbx fg -- '<line>'` runs it");
    outln!("      without ever letting go, and `jbx fg <id>` picks a detached one back");
    outln!("      up. `jbx stats` counts what standing still cost.");
    0
}

/// The informative shape: plain lines, for a hook whose output is read
/// as context rather than as a decision.
pub fn announce_text() -> i32 {
    let pending = take(&client(), "agent");
    if pending.is_empty() {
        return 0; // silence is the normal case, and by far
    }
    for s in &pending {
        let state = if ok(s) {
            "OK".into()
        } else {
            format!("FAILED exit={}", s["code"].as_i64().unwrap_or(-1))
        };
        outln!("[jbx] {} — {state} — {}", name(s), s["log"].as_str().unwrap_or("—"));
    }
    if pending.iter().any(|s| !ok(s)) {
        outln!("A background job failed. Look at its log before stacking anything else on top.");
    }
    0
}

/// The `Stop` shape: JSON, because that is the only hook whose output
/// reaches the model rather than a debug log.
pub fn announce_stop() -> i32 {
    let me = client();
    let pending = take(&me, "agent");
    if pending.is_empty() {
        return 0;
    }
    let summary = pending
        .iter()
        .map(|s| {
            if ok(s) {
                name(s)
            } else {
                format!("{} (exit={})", name(s), s["code"].as_i64().unwrap_or(-1))
            }
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let failed: Vec<&Value> = pending.iter().filter(|s| !ok(s)).collect();
    let what = if pending.len() == 1 {
        "one job finished".to_string()
    } else {
        format!("{} jobs finished", pending.len())
    };
    let mut out = serde_json::json!({
        "systemMessage": format!(
            "jbx: {what} — {summary}.{}",
            if failed.is_empty() { String::new() } else { format!(" {} failed.", failed.len()) }
        )
    });

    // BLOCKING IS THE ONLY WAY IN, and it is spent on failures alone:
    // blocking on every ending would make a session unstoppable.
    //
    // AND ONLY ON FAILURES THIS SESSION CAUSED. The person's mailbox is
    // shared on purpose — one human wants every ending — but announcing
    // is one thing and BLOCKING is another: it holds a session open and
    // sends the model to fix something. Doing that for a job somebody
    // else started sends an agent to read a log from a project it is not
    // working on. Measured the day it happened.
    let mine: Vec<&&Value> = failed
        .iter()
        .filter(|s| s["client"].as_str() == Some(me.as_str()))
        .collect();
    if !mine.is_empty() {
        let logs = mine
            .iter()
            .map(|s| s["log"].as_str().unwrap_or("—"))
            .collect::<Vec<_>>()
            .join(" ");
        out["decision"] = Value::String("block".into());
        out["reason"] = Value::String(format!(
            "jbx: {summary}. Failed job logs: {logs}. Read them, say what broke, \
             and fix it if it is within reach."
        ));
    }
    outln!("{out}");
    0
}
