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
pub struct Record {
    pub id: String,
    /// `true` for a job handed to `queue`, which waits for a slot and is
    /// announced whatever its duration. A wrapped line is neither.
    pub queued: bool,
    pub pid: u32,
    pub command: String,
    pub intent: String,
    pub started: f64,
    pub client: String,
    pub cwd: String,
}

/// What a record looks like right now, which is not stored anywhere: it
/// is read from the files each time, so nothing has to be kept in step.
pub enum State {
    /// HANDED OVER, NOT YET STARTED. Only a `queue` job is ever here: a
    /// wrapped line is already running before this tool has an opinion.
    Queued,
    Running { for_secs: f64 },
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
pub fn intent_of(line: &str) -> String {
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
        "queued": r.queued,
    });
    fs::write(record_path(&r.id), value.to_string())
}

pub fn read_record(id: &str) -> Option<Record> {
    let raw = fs::read_to_string(record_path(id)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(Record {
        id: v["id"].as_str()?.to_string(),
        queued: v["queued"].as_bool().unwrap_or(false),
        pid: v["pid"].as_u64().unwrap_or(0) as u32,
        command: v["command"].as_str().unwrap_or("").to_string(),
        intent: v["intent"].as_str().unwrap_or("").to_string(),
        started: v["started"].as_f64().unwrap_or(0.0),
        client: v["client"].as_str().unwrap_or("default").to_string(),
        cwd: v["cwd"].as_str().unwrap_or("").to_string(),
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

pub fn state_of(r: &Record) -> State {
    if let Ok(text) = fs::read_to_string(code_path(&r.id)) {
        if let Ok(code) = text.trim().parse::<i32>() {
            return State::Finished { code };
        }
    }
    if r.queued && !started_path(&r.id).exists() {
        return State::Queued;
    }
    if alive(r.pid) {
        State::Running { for_secs: now() - r.started }
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
    std::path::Path::new(&format!("/proc/{pid}")).exists()
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
