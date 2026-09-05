//! WHERE A DETACHED LINE LEAVES ITS TRACES, and how it is found again.
//!
//! Three files per wrapped line, all named by its id: `<id>.log` holds
//! everything it printed, `<id>.json` what it was, `<id>.code` its exit
//! status once it has one. THE PRESENCE OF `.code` IS WHAT "FINISHED"
//! MEANS — not a process that is gone, because a process can vanish
//! without leaving anything, and the two have to be told apart.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Everything remembered about one wrapped line.
#[derive(Clone)]
pub struct Record {
    pub id: String,
    /// `true` for a job handed to `queue`, which waits for a slot and is
    /// announced whatever its duration. A wrapped line is neither.
    pub queued: bool,
    /// WHETHER THE LAUNCHER HAS LET GO YET.
    ///
    /// `running` said nothing about the difference between a line still
    /// held — output mirroring to whoever asked, and it may yet finish
    /// in time and vanish — and one that has been let go of, where only
    /// the log receives anything. They are not the same situation and
    /// they do not want the same next move.
    /// `None` when the record predates this field, and it stays `None`
    /// rather than becoming `false`: a record written by an older
    /// version has not told us, and answering "foreground" for it is
    /// asserting something nobody observed. Seen live — a job that had
    /// been let go of fifteen minutes earlier read as held.
    pub detached: Option<bool>,
    /// WHETHER WHOEVER WAS READING THE LAUNCHER WENT AWAY EARLY.
    ///
    /// Kept on the job because there may be nowhere else to say it: with
    /// `… 2>&1 | head -3` both streams are the closed pipe, and nothing
    /// the launcher prints can reach anybody. The record outlives that.
    pub mirror_cut: bool,
    pub pid: u32,
    pub command: String,
    pub intent: String,
    pub started: f64,
    pub client: String,
    pub cwd: String,
    /// THE PROJECT THIS BELONGS TO — the directory of the Claude Code
    /// that ran it, not the directory the command happened to stand in.
    ///
    /// Kept on the job because a list wants to show one project's work,
    /// and two sessions open on the same project are working on the same
    /// thing: scoping by session would hide half of it from each.
    pub project: String,
}

/// What a record looks like right now, which is not stored anywhere: it
/// is read from the files each time, so nothing has to be kept in step.
pub enum State {
    /// HANDED OVER, NOT YET STARTED. Only a `queue` job is ever here: a
    /// wrapped line is already running before this tool has an opinion.
    Queued,
    Running { for_secs: f64, detached: Option<bool> },
    Finished { code: i32 },
    /// GONE WITHOUT A CODE — killed, or the machine went down under it.
    /// Named rather than shown as running forever, and never given a
    /// code of its own: inventing one would be indistinguishable from
    /// the command having really returned it.
    Lost,
}

pub fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// The directory the traces live in.
///
/// `JBX_DIR` FIRST, because the caller is the one who knows where
/// they want their files — and because the tests need somewhere that is
/// not the user's real home. Otherwise the platform's usual place.
pub fn dir() -> PathBuf {
    root().join("jobs")
}

/// WHAT `JBX_DIR` NAMES: the directory everything lives UNDER, not the
/// one jobs land in. `config` reports settings, and reporting a computed
/// subdirectory where a setting belongs invites somebody to set the
/// variable to it — which would then nest one level deeper every time.
pub fn root() -> PathBuf {
    crate::config::dir(platform_root()).0
}

/// Where the platform would put a cache, when nobody has said otherwise.
fn platform_root() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache"));
    base.unwrap_or_else(|| PathBuf::from(".")).join("jbx")
}

pub fn log_path(id: &str) -> PathBuf { dir().join(format!("{id}.log")) }
pub fn code_path(id: &str) -> PathBuf { dir().join(format!("{id}.code")) }
pub fn record_path(id: &str) -> PathBuf { dir().join(format!("{id}.json")) }
/// WRITTEN THE MOMENT A QUEUED JOB GETS ITS SLOT. Its absence is
/// what "still waiting its turn" means — a state that has to be
/// readable from outside, by a `list` that never spoke to the
/// supervisor holding the job.
pub fn started_path(id: &str) -> PathBuf { dir().join(format!("{id}.started")) }

/// A NEW ID, SHORT AND UNMISTAKABLE.
///
/// Seven hex characters behind a `j`, so it cannot be read as a number
/// and cannot collide with anything a queue numbers sequentially. Drawn
/// from the clock and the process id rather than a random generator: a
/// dependency for seven characters is a dependency to keep up to date
/// forever, and two ids only have to differ, not be unguessable.
pub fn mint() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ (d.as_secs() << 20))
        .unwrap_or(0);
    format!("j{:07x}", (nanos ^ (std::process::id() as u64) << 13) & 0xfff_ffff)
}

/// A NAME FOR A LINE NOBODY NAMED.
///
/// Nothing wrapped a command with an intent in mind — it was wrapped
/// because it turned out to be slow — so the name has to come from the
/// line itself. The first few words are what a reader recognises three
/// hours later, and never a hash: this is read by people.
/// Whether a stored name was read off the line rather than given.
///
/// It is one when it is the HEAD OF ITS OWN COMMAND — four words that
/// say nothing the line does not already say. A sentence somebody wrote
/// is not, which is the whole distinction. Every past shape of the rule
/// is tried, because that is exactly what an old record holds.
fn was_derived(command: &str, stored: &str) -> bool {
    let head = flat(stored.trim_end_matches('…'));
    if head.is_empty() {
        return true;
    }
    let command = flat(command);
    [
        command.as_str(),
        crate::stats::without_leading_cd(&command),
        crate::stats::without_preamble(&command),
    ]
    .iter()
    .any(|shape| shape.starts_with(&head))
}

/// One line, single-spaced, so two spellings of the same words compare.
fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn intent_of(line: &str) -> String {
    // THE WRAPPERS ARE NOT THE NAME OF THE WORK. A line arrives inside
    // `cd <root> &&` from the harness and often `timeout <n>` and `rtk
    // proxy` besides, so the first four words named the envelope —
    // `cd /home/…/bms && t…`, then `timeout 300 rtk proxy`. Neither says
    // what is running.
    let line = crate::stats::without_preamble(line);
    let short: String = line.split_whitespace().take(4).collect::<Vec<_>>().join(" ");
    if short.chars().count() > 58 {
        short.chars().take(57).collect::<String>() + "…"
    } else if short.is_empty() {
        "(empty)".into()
    } else {
        short
    }
}

/// WHO IS ASKING. One definition, in `signals`, because the name is a
/// mailbox address before it is anything else — and two ways of spelling
/// one address is how endings come to be deposited where nobody reads.
pub fn client() -> String {
    crate::signals::client()
}

pub fn write_record(r: &Record) -> io::Result<()> {
    let value = serde_json::json!({
        "id": r.id, "pid": r.pid, "command": r.command, "intent": r.intent,
        "started": r.started, "client": r.client, "cwd": r.cwd,
        "queued": r.queued, "mirror_cut": r.mirror_cut, "detached": r.detached,
        "project": r.project,
    });
    fs::write(record_path(&r.id), value.to_string())
}

pub fn read_record(id: &str) -> Option<Record> {
    let raw = fs::read_to_string(record_path(id)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(Record {
        id: v["id"].as_str()?.to_string(),
        queued: v["queued"].as_bool().unwrap_or(false),
        mirror_cut: v["mirror_cut"].as_bool().unwrap_or(false),
        detached: v["detached"].as_bool(),
        pid: v["pid"].as_u64().unwrap_or(0) as u32,
        command: v["command"].as_str().unwrap_or("").to_string(),
        // A DERIVED NAME IS A RENDERING, NOT A RECORDING.
        //
        // It was stored as it was computed, so every record kept the
        // name whichever binary wrote it happened to derive — a store
        // read on one afternoon held three generations of the rule at
        // once, and the older rows named `cd /home/…` for ever. What the
        // caller SAID is data and is kept; what was read off the line is
        // read off the line again, here, and improves with the rule.
        intent: {
            let command = v["command"].as_str().unwrap_or("");
            let stored = v["intent"].as_str().unwrap_or("");
            if was_derived(command, stored) { intent_of(command) } else { stored.to_string() }
        },
        started: v["started"].as_f64().unwrap_or(0.0),
        client: v["client"].as_str().unwrap_or("default").to_string(),
        cwd: v["cwd"].as_str().unwrap_or("").to_string(),
        // A RECORD FROM BEFORE THIS FIELD reconstructs it from where it
        // stood, which is what the project used to be derived from
        // anyway — a fair reading rather than a blank.
        project: v["project"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| {
                crate::config::project_root_of(std::path::Path::new(
                    v["cwd"].as_str().unwrap_or("."),
                ))
                .display()
                .to_string()
            }),
    })
}

/// THE EXIT CODE IS WRITTEN WHOLE OR NOT AT ALL.
///
/// Written to a neighbouring name and then renamed, because `rename` is
/// the one filesystem operation that is atomic on both platforms. A
/// reader polling for `<id>.code` would otherwise be able to open it
/// after it was created and before the digits were in it, and read a
/// finished job with no code — the one state that means "it was killed".
pub fn write_code(id: &str, code: i32) -> io::Result<()> {
    let tmp = dir().join(format!("{id}.code.part"));
    fs::write(&tmp, code.to_string())?;
    fs::rename(&tmp, code_path(id))
}

/// The same, but never concluding LOST from a single look.
///
/// "Gone without leaving an exit code" is the one state a caller acts
/// on — `wait` and `fg` return failure for it — and it is also the one
/// that a race produces out of nothing: a supervisor between its last
/// write and its exit is momentarily neither running nor recorded. On a
/// slow machine that window is wide enough to be seen, which is how two
/// macOS runs disagreed about the same code.
///
/// So the absence has to persist. Anything else is answered at once;
/// only the alarming answer is asked twice.
pub fn settled_state(r: &Record) -> State {
    match state_of(r) {
        State::Lost => {
            std::thread::sleep(std::time::Duration::from_millis(250));
            state_of(r)
        }
        other => other,
    }
}

pub fn state_of(r: &Record) -> State {
    if let Ok(text) = fs::read_to_string(code_path(&r.id)) {
        if let Ok(code) = text.trim().parse::<i32>() {
            return State::Finished { code };
        }
    }
    // WAITING ITS TURN ONLY COUNTS IF SOMETHING IS STILL WAITING. This
    // branch used to answer before the liveness check, so a queued job
    // whose supervisor had been killed read "waiting for a slot" for
    // ever — and `wait` on it blocked for ever with it. Stopping a job
    // that has not started is a legitimate thing to do, and the state it
    // leaves has to say so.
    if r.queued && !started_path(&r.id).exists() {
        return if alive(r.pid) { State::Queued } else { State::Lost };
    }
    if alive(r.pid) {
        State::Running { for_secs: now() - r.started, detached: r.detached }
    } else {
        State::Lost
    }
}

/// Whether a detached supervisor is still there.
///
/// It can be fooled by pid reuse, which is why it is never read alone: a
/// finished job is recognised by its RECORDED CODE, and this only
/// answers for one that left none.
#[cfg(target_os = "linux")]
pub fn alive(pid: u32) -> bool {
    // A ZOMBIE STILL HAS A `/proc` ENTRY, and existence was the whole
    // test. A process whose parent has gone is reparented to init and
    // reaped at once on an ordinary machine — but inside a container
    // whose pid 1 reaps nothing, a killed supervisor stays listed for
    // ever. The job then reads `queued` or `running` long after it was
    // stopped, which is exactly the failure the CI runners showed and
    // this machine could not.
    let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    // The state letter follows the closing parenthesis of the name,
    // which is where it has to be read from: a process can be called
    // `weird ) name`.
    match stat.rsplit_once(") ") {
        Some((_, rest)) => rest.split_whitespace().next() != Some("Z"),
        None => true,
    }
}

/// The other Unixes have no `/proc`, so they are asked with the tool
/// they all ship.
///
/// THIS WAS `cfg(unix)` AND READ `/proc` EVERYWHERE, which on macOS and
/// the BSDs is not a missing answer but a WRONG one: every finished job
/// would have read as "gone, no exit code" — the state that means it was
/// killed. A platform that cannot answer must say so, never guess, and
/// least of all guess the alarming value.
///
/// It costs a process per call, where Linux costs a `stat`. `state_of`
/// asks once per record, so a long `list` pays for it; that is the price
/// of not carrying a C dependency for one boolean.
#[cfg(all(unix, not(target_os = "linux")))]
pub fn alive(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(true)
}

#[cfg(windows)]
pub fn alive(pid: u32) -> bool {
    // No /proc, and no unsafe call to a Win32 API for one boolean: ask
    // the tool Windows ships for exactly this question. It prints the
    // process when it exists and an error when it does not.
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(true)
}

/// BEFORE A RUNNING JOB IS CALLED MUTE.
///
/// Ten minutes, and it is a preference, so it is a setting.
pub fn mute_after() -> f64 {
    crate::config::mute_after().0
}

/// How many seconds since this job wrote anything, or `None`.
///
/// LIVENESS IS READ FROM FILE FRESHNESS, not from a heartbeat the script
/// would have to emit. A heartbeat only ever works for scripts we write,
/// and whoever forgets it looks dead. Freshness rewards exactly what a
/// well-made script already does: the one that says where it is at gets
/// precise liveness for free, and the silent one gets "I do not know",
/// which is the honest answer.
pub fn silence(r: &Record) -> Option<f64> {
    if !matches!(state_of(r), State::Running { .. }) {
        return None;
    }
    let modified = fs::metadata(log_path(&r.id)).ok()?.modified().ok()?;
    let age = SystemTime::now().duration_since(modified).ok()?;
    Some(age.as_secs_f64())
}

/// WHICH OF THESE ARE STILL THERE — asked once for all of them.
///
/// `alive` is a `stat` on Linux and a PROCESS on Windows, where it shells
/// out to `tasklist`. The queue asks about every outstanding ticket on
/// every turn of a 200 ms wait, so ten waiters meant ten `tasklist`
/// launches five times a second — bearable where the answer is a `stat`,
/// and not where it is a process.
///
/// One listing answers for everybody, so the cost stops growing with the
/// length of the line.
pub fn alive_many(pids: &[u32]) -> std::collections::HashSet<u32> {
    #[cfg(target_os = "linux")]
    {
        pids.iter().copied().filter(|p| alive(*p)).collect()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let listing = live_pids();
        match listing {
            Some(live) => pids.iter().copied().filter(|p| live.contains(p)).collect(),
            // COULD NOT ASK: everything is assumed alive. Reclaiming a
            // ticket that is still held would let somebody jump the
            // line; leaving a dead one costs a wait that the next
            // successful listing ends.
            None => pids.iter().copied().collect(),
        }
    }
}

#[cfg(windows)]
fn live_pids() -> Option<std::collections::HashSet<u32>> {
    let out = std::process::Command::new("tasklist")
        .args(["/NH", "/FO", "CSV"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    // "name","pid","session","#","mem" — the second field, and the
    // quoting is what makes it parseable without knowing the language
    // the rest of the line is written in.
    Some(
        text.lines()
            .filter_map(|l| l.split("\",\"").nth(1))
            .filter_map(|f| f.trim_matches('"').parse().ok())
            .collect(),
    )
}

#[cfg(all(unix, not(target_os = "linux")))]
fn live_pids() -> Option<std::collections::HashSet<u32>> {
    let out = std::process::Command::new("ps").args(["-A", "-o", "pid="]).output().ok()?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .filter_map(|f| f.parse().ok())
            .collect(),
    )
}

pub fn all() -> Vec<Record> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir()) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if let Some(r) = read_record(stem) {
                out.push(r);
            }
        }
    }
    out.sort_by(|a, b| a.started.total_cmp(&b.started));
    out
}

/// DROP THE TRACES OF LINES THAT ENDED LONG AGO.
///
/// A wrapper that wraps every command leaves files every time, so this
/// is not housekeeping: it is what stops `list` from being unreadable by
/// the end of a week. A job still running is never swept, however old —
/// age is not a reason to forget something that is still happening.
pub fn forget_older_than(hours: f64) -> usize {
    let cut = now() - hours * 3600.0;
    let mut gone = 0;
    for r in all() {
        if r.started > cut || !code_path(&r.id).exists() {
            continue;
        }
        for path in [
            record_path(&r.id),
            code_path(&r.id),
            log_path(&r.id),
            started_path(&r.id),
        ] {
            let _ = fs::remove_file(path);
        }
        gone += 1;
    }
    gone
}
