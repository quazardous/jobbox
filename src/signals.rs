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
use std::path::PathBuf;

use serde_json::Value;

use crate::{gain, store};

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

/// WHERE THIS SESSION STARTED, remembered the first time we are asked.
///
/// A session's working directory MOVES — `cd` in one command changes it
/// for every command after — so filing a reading by it splits one
/// session's time across whatever it walked through. Measured on a real
/// store: a session began at a repository root, moved into a
/// sub-project, and its first row froze at the minute it moved while a
/// second row started filling.
///
/// Where Claude Code STARTED does not move, and it is what somebody
/// means by "which project was I working on". Only the hook can see it:
/// `CLAUDE_PROJECT_DIR` is given to hooks and not to commands, and the
/// event carries a `cwd` besides. So the hook writes it down once, and
/// everything else looks it up by the session id — which every process
/// does have.
///
/// WRITTEN ONCE AND NEVER UPDATED. A later `cd` must not move it; that
/// is the whole point of preferring it to the working directory.
pub fn remember_session_root(event: &Value) {
    let Some(session) = session_id() else { return };
    let dir = std::env::var_os("CLAUDE_PROJECT_DIR")
        .map(PathBuf::from)
        .or_else(|| event["cwd"].as_str().map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok());
    let Some(dir) = dir else { return };
    let path = roots_dir().join(&session);
    if path.exists() {
        return;
    }
    let _ = fs::create_dir_all(roots_dir());
    let _ = fs::write(path, dir.display().to_string());
}

/// Where this session started, if the hook has told us.
pub fn session_root() -> Option<PathBuf> {
    let session = session_id()?;
    let text = fs::read_to_string(roots_dir().join(session)).ok()?;
    let dir = PathBuf::from(text.trim());
    dir.is_dir().then_some(dir)
}

fn roots_dir() -> PathBuf {
    store::dir().join("sessions")
}

fn session_id() -> Option<String> {
    // LONGER THAN THE MAILBOX NAME, and deliberately: this one names a
    // directory, where a collision costs more than a few characters.
    // Same reading of the environment, rendered differently.
    let raw = crate::harness::session_raw()?;
    let clean: String = raw.chars().take(16).filter(|c| c.is_ascii_alphanumeric()).collect();
    (!clean.is_empty()).then_some(clean)
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
    // WHICH HARNESS, AND ITS SESSION — asked in one place, so a client
    // added there gets its own mailbox without anybody editing this.
    if let Some(name) = crate::harness::session() {
        return name;
    }
    // NO SESSION: a plain shell. Two terminals in one project then share
    // a mailbox, which is right — the person wants every ending, and the
    // shared box already works that way.
    let (project, _) = gain::project();
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
        // ONE WRITE, because the supervisor of every other job ending at
        // this instant is appending to the same box. See `store::append_line`.
        store::append_line(&box_path, &line);
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

/// HOW LONG A MAILBOX MAY SIT UNTOUCHED BEFORE IT COUNTS AS ABANDONED.
///
/// Deliberately generous. Getting this wrong in one direction costs a
/// stale box a few more hours of existence; in the other it takes the
/// mail of a session that is merely quiet. Six hours is longer than any
/// pause inside a working session and shorter than the gap between days.
const ABANDONED_AFTER: f64 = 6.0 * 3600.0;

/// CLEAR OUT MAILBOXES WHOSE READER IS GONE.
///
/// `jbx health` has always been able to LIST these — "endings addressed
/// to sessions that are gone" — and listing was all it did, so the list
/// could only grow. An alarm that always rings is one nobody reads.
///
/// NOTHING IS LOST, AND IT IS CHECKED RATHER THAN ASSUMED. Every ending
/// is deposited to two boxes at once: the session's, and the shared one
/// the person reads. So a stale session box holds copies — measured
/// across five real boxes, every single ending was already in the shared
/// one. This still looks, and carries over anything that is not there,
/// because "it was true when I looked" is not a reason to delete.
///
/// BY AGE, NOT BY "NOT MINE". `stranded()` calls every box that is not
/// ours stranded, which is fine for showing and wrong for taking: two
/// sessions open at once, and each would empty the other's mail before
/// it was read.
///
/// CALLED FROM THE VERBS THAT ALREADY LOOK — `health`, `list`, `ps`,
/// `gain` — and never from `run` or `hook`. Those wrap every command on
/// the machine; making all of them pay to read a directory for a tidy-up
/// nobody asked for at that moment is the wrong trade.
pub fn sweep() -> usize {
    let me = client();
    let now = store::now();
    let Ok(entries) = fs::read_dir(signals_dir()) else { return 0 };
    let mut swept = 0;
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let who = entry.file_name().to_string_lossy().into_owned();
        if who == me {
            continue;
        }
        let theirs = entry.path().join("agent.jsonl");
        let idle = fs::metadata(&theirs)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        let _ = now;
        if idle < ABANDONED_AFTER {
            continue;
        }
        let raw = fs::read_to_string(&theirs).unwrap_or_default();
        let waiting: Vec<Value> = raw
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        // WHAT THE PERSON HAS NOT GOT IS CARRIED OVER FIRST. Only then
        // does the box go, so an interrupted sweep leaves a duplicate at
        // worst and never a hole.
        let shared = mailbox(&who, "user");
        let already: std::collections::BTreeSet<String> = fs::read_to_string(&shared)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter_map(|v| v["id"].as_str().map(str::to_string))
            .collect();
        let orphans: Vec<&Value> = waiting
            .iter()
            .filter(|s| !s["id"].as_str().map(|i| already.contains(i)).unwrap_or(false))
            .collect();
        if !orphans.is_empty() {
            // ONE WRITE FOR THE WHOLE BATCH, into a box supervisors may be
            // appending to at the same moment. See `store::append_line`.
            let batch: Vec<String> = orphans.iter().map(|s| s.to_string()).collect();
            store::append_line(&shared, &batch.join("\n"));
        }
        let _ = fs::remove_dir_all(entry.path());
        swept += waiting.len();
    }
    swept
}

/// DROP ONE ENDING THAT HAS ALREADY BEEN DELIVERED, and only that one.
///
/// `jbx wait <id>` blocks until a job ends and exits with its code — so
/// by the time it returns, the ending HAS reached whoever asked. The
/// message announcing it has no recipient left, and leaving it in the
/// box makes `jbx health` report an ending nobody is waiting for.
///
/// THE AGENT'S BOX ONLY. The shared box is the person's mail, read in a
/// terminal at their own pace; an agent waiting on a job is no reason to
/// throw away what a human has not seen.
///
/// TARGETED, NEVER `take`. Emptying the whole box because one job was
/// waited on would discard the endings of every other job in it — the
/// ones that have NOT been delivered, which are exactly the ones that
/// matter.
///
/// A BOX IS ADDRESSED TO A CLIENT, NOT TO A PAIR OF EYES, and that
/// leaves one narrow asymmetry — named here rather than found later. If
/// a person runs `jbx wait` under the same client as an agent, and an
/// announcing hook was going to tell that agent, the person's wait takes
/// the message first. Narrow, because the default install declares no
/// announcing hook and so has nothing to take; and it is the price of
/// addressing a box to a session.
///
/// IT CLAIMS BY RENAMING, like `take`, so an ending deposited mid-edit
/// is not erased under it. What arrives during the rewrite lands in a
/// fresh file and is left alone; the survivors are appended after it,
/// which reorders nothing that is read in order.
pub fn forget(client: &str, id: &str) -> bool {
    let path = mailbox(client, "agent");
    if !path.exists() {
        return false;
    }
    let claimed = path.with_extension(format!("jsonl.forgetting-{}", std::process::id()));
    if fs::rename(&path, &claimed).is_err() {
        return false;
    }
    let raw = fs::read_to_string(&claimed).unwrap_or_default();
    let _ = fs::remove_file(&claimed);
    let mut dropped = false;
    let kept: Vec<&str> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter(|l| {
            let mine = serde_json::from_str::<Value>(l)
                .map(|v| v["id"].as_str() == Some(id))
                .unwrap_or(false);
            dropped |= mine;
            !mine
        })
        .collect();
    if !kept.is_empty() {
        // ONE WRITE FOR THE SURVIVORS, since an ending may be landing in
        // the fresh file right now. See `store::append_line`.
        store::append_line(&path, &kept.join("\n"));
    }
    dropped
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
    // ONCE PER SESSION, ACROSS PROCESSES — not once per registration.
    //
    // The rule was already said once per hook. It is now possible to be
    // declared twice: `jbx init` writes the hooks into the settings file
    // and the plugin declares the same four, so a machine with both runs
    // each event twice, in two processes that know nothing of each
    // other. MEASURED on a real load: the whole discipline printed
    // twice, which is how a paragraph becomes wallpaper.
    //
    // A marker in the store settles it. `create_new` is the atomic
    // gesture on both platforms: whoever creates it speaks, the other
    // finds it there and says nothing.
    let said = store::dir().join(format!("said-{}", crate::signals::client()));
    let _ = std::fs::create_dir_all(store::dir());
    if std::fs::OpenOptions::new().write(true).create_new(true).open(&said).is_err() {
        return 0;
    }
    // TWO BEATS, AND THEY ARE NOT THE SAME INSTRUCTION.
    //
    // The first is a judgement NOT to make: whether a line will be long.
    // Everybody gets it wrong, the tool refuses to make it, and a reader
    // who is not told so will make it anyway — by backgrounding things
    // by hand, or by sitting on a build in case.
    //
    // The second is the only discipline left: a job that has let go
    // finishes whether or not anybody watches it. Waiting on it, and
    // polling it, are the same waste in different clothes.
    outln!("[jbx] Every command here is wrapped.");
    outln!("      DO NOT DECIDE IN ADVANCE whether one will be long. That judgement is");
    outln!("      the thing everybody gets wrong, so this does not make it: it runs the");
    outln!("      line and finds out. Nothing for you to do differently.");
    outln!("      DO NOT WAIT ON A JOB THAT HAS LET GO. It finishes whether you watch it");
    outln!("      or not, and you are told when it does — so go and do something else.");
    outln!("      Polling is waiting with extra steps; with nothing else to do, run");
    outln!("      `jbx wait <id>` as a BACKGROUND command and its ending wakes you.");
    outln!("      Genuinely cannot go on without a result? Say so: `jbx fg -- '<line>'`");
    outln!("      never lets go, and `jbx gain` counts what that cost.");
    0
}

/// The informative shape: plain lines, for a hook whose output is read
/// as context rather than as a decision.
pub fn announce_text() -> i32 {
    let pending = take(&client(), "agent");
    // SAID EVEN WHEN NOTHING ENDED. This used to return here on an empty
    // mailbox, and a job running since this morning is precisely the
    // case where nothing ends: the whole point is that it never will.
    if pending.is_empty() {
        return warn_about_the_endless();
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
    warn_about_the_endless()
}

/// WHAT JBX IS NOT, SAID TO WHOEVER LEFT SOMETHING RUNNING FOR HOURS.
///
/// jbx wraps a LINE. It keeps that line's log and record for a day and
/// sweeps them after, and a reboot ends every job it knows about without
/// recording anything — thirty-two records were left that way on
/// 12/09/2026, one of them a worker somebody had restarted through jbx.
/// Nothing in the tool says so until something has already been lost.
///
/// PUSHED, NOT SHOWN. `jbx health` answers whoever asks, and the person
/// who needs this is by definition not asking — so it goes out on the
/// hook, in front of the next command. That is also why it stops: once
/// per `warn_again_after` per job, marked in the store, or a true
/// sentence becomes wallpaper by the third repetition.
fn warn_about_the_endless() -> i32 {
    let now = store::now();
    let here = crate::gain::project().1;
    let again = store::warn_again_after();
    let mut called_out = Vec::new();
    for r in store::all() {
        if r.project != here || !store::nobody_is_watching(&r) {
            continue;
        }
        if !matches!(store::state_of(&r), store::State::Running { .. }) {
            continue;
        }
        let ran_for = now - r.started;
        if ran_for < store::allowed_to_run(&r.id) {
            continue;
        }
        if store::warned_at(&r.id).is_some_and(|said| now - said < again) {
            continue;
        }
        store::note_warned(&r.id);
        called_out.push((r.id.clone(), ran_for));
    }
    if called_out.is_empty() {
        return 0;
    }
    for (id, ran_for) in &called_out {
        outln!("[jbx] {id} has been running {}.", store::how_long(*ran_for));
    }
    // THE REASON, NOT THE SCOLDING. What is wrong is not the duration —
    // some work is genuinely long, and there is a verb for saying so.
    // What is wrong is expecting jbx to keep something alive, because it
    // is the one thing it does not do.
    outln!("      jbx runs a line and remembers it for a day. It does not keep one alive:");
    outln!("      a reboot ends it, records and all. Something with no end of its own — a");
    outln!("      daemon, a worker, a watcher — belongs under systemd or docker, not here.");
    outln!("      Genuinely one long piece of work? Say so, and this stops:");
    for (id, _) in &called_out {
        outln!("        jbx expect {id} 4h");
    }
    0
}

/// The `Stop` shape: JSON, because that is the only hook whose output
/// reaches the model rather than a debug log.
/// `hold` IS THE CLIENT'S OWN WORD FOR "do not finish yet", and the
/// clients do not share it: Claude wants `decision: "block"`, Gemini's
/// `AfterAgent` wants `"deny"` with the `reason` sent back as a fresh
/// prompt. Same effect, different spelling — and the wrong one is an
/// unknown value, which is not an error anybody would ever see.
pub fn announce_stop(hold: &str) -> i32 {
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
        out["decision"] = Value::String(hold.into());
        out["reason"] = Value::String(format!(
            "jbx: {summary}. Failed job logs: {logs}. Read them, say what broke, \
             and fix it if it is within reach."
        ));
    }
    outln!("{out}");
    0
}
