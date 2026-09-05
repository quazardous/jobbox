//! `jbx` — ONE BINARY, AND THE SPLIT THAT WAS NOT WORTH IT.
//!
//! Two of these existed for an afternoon: a hot half for what a harness
//! calls on every command, and a `jbxctl` for what a person types. The
//! reasoning was sound and the measurement did not support it — a Rust
//! binary does not build a parser at startup the way an interpreter
//! does, so carrying six more verbs costs nothing you can measure. See
//! the table in the README.
//!
//! What the split DID cost was real: two names to install, two to keep
//! in step, and a `jbxctl` that could not answer the hook it declared.

use std::io::Write;

use jobbox::{default_after, hook, init, run, signals, slots, stats, store, tail};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(dispatch(args));
}

fn dispatch(args: Vec<String>) -> i32 {
    let verb = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };

    match verb {
        "-V" | "--version" => {
            jobbox::outln!("jbx {VERSION}");
            0
        }
        "" | "-h" | "--help" | "help" => {
            print!("{}", usage());
            0
        }
        "run" => {
            let mut after = default_after();
            let mut i = 0;
            while i < rest.len() && rest[i] != "--" {
                if rest[i] == "--after" && i + 1 < rest.len() {
                    if let Ok(v) = rest[i + 1].parse() {
                        after = v;
                    }
                    i += 2;
                } else {
                    i += 1;
                }
            }
            run::run(after, &tail(rest))
        }
        // NOT IN THE HELP: it is one half of this binary talking to the
        // other, and a verb a person can be tempted to type by hand is a
        // verb that will be typed by hand.
        "supervise" => match rest.first() {
            Some(id) => {
                let after = rest
                    .iter()
                    .position(|a| a == "--after")
                    .and_then(|i| rest.get(i + 1))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(default_after);
                run::supervise(
                    id,
                    after,
                    rest.iter().any(|a| a == "--queued"),
                    rest.iter().any(|a| a == "--fg"),
                    &tail(&rest[1..]),
                )
            }
            None => 2,
        },
        "hook" => {
            let binary = std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "jbx".into());
            hook::hook(&binary)
        }
        // THE INTENT COMES FIRST AND THE LINE AFTER `--`, so a name
        // with spaces is impossible to confuse with the command.
        // `--` IS NOT A NAME. Taken as one it queued a job called "--",
        // which is exactly the unnamed job this verb exists to refuse —
        // and the first test written for it found that.
        // `fg` TAKES EITHER, AND THE TWO CANNOT BE CONFUSED: an id is
        // `j` followed by seven hex digits and nothing else, which no
        // command is. A `--` settles it either way.
        "fg" => match rest.first() {
            Some(first) if looks_like_an_id(first) && rest.len() == 1 => run::attach(first),
            Some(_) => run::foreground(&tail(rest)),
            None => usage_error("fg needs a line or a job id"),
        },
        "queue" => match rest.first() {
            Some(intent) if intent != "--" && rest.len() > 1 => {
                run::queue(intent, &tail(&rest[1..]))
            }
            _ => usage_error("queue needs an intent and a line: `jbx queue build -- make`"),
        },
        "slots" => slots_cmd(rest.first().map(String::as_str)),
        "health" => health(),
        "clients" => clients(),
        "config" => config(),
        "signals" => match rest.first() {
            Some(audience) => signals::signals(
                audience,
                rest.iter().any(|a| a == "--json"),
                rest.iter()
                    .position(|a| a == "--client")
                    .and_then(|i| rest.get(i + 1))
                    .map(String::as_str),
            ),
            None => usage_error("signals needs an audience: agent or user"),
        },
        "stats" => stats::stats(rest.first().filter(|a| !a.starts_with('-')).map(String::as_str)),
        "init" => init::init(rest.iter().any(|a| a == "--undo")),
        "list" => list(),
        "status" => match rest.first() {
            Some(id) => status(id),
            None => usage_error("status needs an id"),
        },
        "tail" => match rest.first() {
            Some(id) => tail_log(id, rest.iter().any(|a| a == "-f")),
            None => usage_error("tail needs an id"),
        },
        "wait" => match rest.first() {
            Some(id) => wait(id),
            None => usage_error("wait needs an id"),
        },
        "kill" => match rest.first() {
            Some(id) => kill(id),
            None => usage_error("kill needs an id"),
        },
        other => {
            eprintln!("jbx: unknown verb {other:?}");
            eprint!("{}", usage());
            2
        }
    }
}

fn usage() -> String {
    format!(
        "jbx {VERSION} — run a line, and detach it if it turns out to be long.\n\
         \n\
         \x20 jbx run -- '<line>'   run it, detaching after {:.0}s\n\
         \x20 jbx fg -- '<line>'    run it and NEVER let go — say so on purpose\n\
         \x20 jbx fg <id>           bring a detached job back to the foreground\n\
         \x20 jbx queue <intent> -- '<line>'\n\
         \x20                       hand it over before it starts, and name it\n\
         \x20 jbx hook              the PreToolUse hook, called by a harness\n\
         \n\
         \x20 jbx list              what is detached, and how it went\n\
         \x20 jbx status <id>       state, exit code, where its log is\n\
         \x20 jbx tail <id> [-f]    what it printed\n\
         \x20 jbx wait <id>         block until it ends, exit with its code\n\
         \x20 jbx kill <id>         stop it, and everything it started\n\
         \x20 jbx slots [n|none]    how many queued jobs may run at once\n\
         \x20 jbx signals <who>     endings not yet read: agent or user\n\
         \x20 jbx stats [project]   what takes time, per project\n\
         \x20 jbx health            what runs, what is mute, what is stranded\n\
         \x20 jbx clients           whose endings are still unread\n\
         \x20 jbx config            every setting, and where it came from\n\
         \x20 jbx init [--undo]     declare the hook, and displace rtk's\n\
         \n\
         JBX_AFTER   seconds before detaching (now {:.0})\n\
         JBX_DIR          where logs and records live (now {})\n",
        default_after(),
        default_after(),
        jobbox::store::dir().display()
    )
}


fn usage_error(what: &str) -> i32 {
    eprintln!("jbx: {what}");
    eprint!("{}", usage());
    2
}

fn describe(state: &store::State) -> String {
    match state {
        // "QUEUED IS NOT STUCK." It is waiting its turn, and saying which
        // is the difference between someone leaving it alone and someone
        // going to look for a fault that is not there.
        store::State::Queued => "queued    waiting for a slot".into(),
        store::State::Running { for_secs } => format!("running   {for_secs:.0}s"),
        store::State::Finished { code } => format!("finished  exit {code}"),
        store::State::Lost => "gone      no exit code — killed, or the machine went down".into(),
    }
}

fn list() -> i32 {
    let records = store::all();
    if records.is_empty() {
        jobbox::outln!("nothing detached.");
        return 0;
    }
    jobbox::outln!("{:<10} {:<48} {:<12} line", "id", "state", "");
    for r in &records {
        // MUTENESS IS ONLY SAID WHEN IT MATTERS. On every line it would
        // be a column people stop reading — and it is precisely the one
        // that must be seen the day it speaks.
        let mute = match store::silence(r) {
            Some(secs) if secs > store::mute_after() => format!("MUTE {}s", secs as i64),
            _ => String::new(),
        };
        jobbox::outln!("{:<10} {:<48} {:<12} {}", r.id, describe(&store::state_of(r)), mute, r.intent);
    }
    0
}

fn status(id: &str) -> i32 {
    let Some(r) = store::read_record(id) else {
        eprintln!("jbx: {id} is unknown");
        return 1;
    };
    let state = store::state_of(&r);
    jobbox::outln!("  id       {}", r.id);
    jobbox::outln!("  state    {}", describe(&state));
    jobbox::outln!("  line     {}", r.command);
    jobbox::outln!("  client   {}", r.client);
    jobbox::outln!("  where    {}", r.cwd);
    jobbox::outln!("  log      {}", store::log_path(&r.id).display());
    // THE JOB'S CODE BECOMES OURS, so a script can decide without
    // reading a word of this.
    match state {
        store::State::Finished { code: 0 } => 0,
        store::State::Finished { .. } => 1,
        _ => 0,
    }
}

fn tail_log(id: &str, follow: bool) -> i32 {
    use std::io::{Read, Seek, SeekFrom};
    let path = store::log_path(id);
    let Ok(mut file) = std::fs::File::open(&path) else {
        eprintln!("jbx: {id} has no log");
        return 1;
    };
    let mut text = String::new();
    let _ = file.read_to_string(&mut text);
    print!("{text}");
    let _ = std::io::stdout().flush();
    if !follow {
        return 0;
    }
    let mut at = file.stream_position().unwrap_or(0);
    loop {
        if let Some(r) = store::read_record(id) {
            let ended = matches!(
                store::state_of(&r),
                store::State::Finished { .. } | store::State::Lost
            );
            let _ = file.seek(SeekFrom::Start(at));
            let mut more = String::new();
            let _ = file.read_to_string(&mut more);
            at += more.len() as u64;
            print!("{more}");
            let _ = std::io::stdout().flush();
            if ended {
                return 0;
            }
        } else {
            return 0;
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
}

/// BLOCK UNTIL IT ENDS, and leave with its exit code.
///
/// THIS IS THE ANSWER TO "AND HOW DO I WAIT". A detached job that could
/// only be polled pushes every caller into writing the same sleep loop,
/// each one slightly wrong. The message printed at detachment names this
/// verb, so the thing it tells you to do has to exist.
fn wait(id: &str) -> i32 {
    let began = store::now();
    loop {
        let Some(r) = store::read_record(id) else {
            eprintln!("jbx: {id} is unknown");
            return 1;
        };
        match store::state_of(&r) {
            // THE BLOCK IS WRITTEN DOWN BEFORE LEAVING, always. Time
            // handed back to a wait is time the wrapper did not save,
            // and a tool that forgot to subtract it would report its own
            // good intentions as a result.
            store::State::Finished { code } => {
                stats::record_wait(store::now() - began);
                return code;
            }
            store::State::Lost => {
                stats::record_wait(store::now() - began);
                eprintln!("jbx: {id} ended without leaving an exit code");
                return 1;
            }
            store::State::Queued | store::State::Running { .. } => {
                std::thread::sleep(std::time::Duration::from_millis(200))
            }
        }
    }
}

/// Stop a detached line — and everything it started.
///
/// THE WHOLE GROUP GOES, not just the supervisor. A line is usually a
/// shell that started something else; killing the shell alone would
/// leave the real work running with nothing watching it.
fn kill(id: &str) -> i32 {
    let Some(r) = store::read_record(id) else {
        eprintln!("jbx: {id} is unknown");
        return 1;
    };
    #[cfg(unix)]
    let done = std::process::Command::new("kill")
        .args(["-TERM", &format!("-{}", r.pid)])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    #[cfg(windows)]
    let done = std::process::Command::new("taskkill")
        .args(["/T", "/F", "/PID", &r.pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if done {
        // SAID OUT LOUD, because the code will not say it: a killed line
        // leaves an interrupt code that a later reader would take for a
        // failure of the command itself.
        eprintln!("jbx: {id} stopped");
        0
    } else {
        eprintln!("jbx: could not stop {id} (pid {})", r.pid);
        1
    }
}

/// `jbx slots [n]` — READ THE CAP, OR SET IT.
///
/// It governs `queue` alone. A wrapped line is already running by the
/// time this tool sees it, so capping those would cap nothing — the
/// message says so rather than letting a number imply otherwise.
fn slots_cmd(value: Option<&str>) -> i32 {
    if let Some(value) = value {
        if value != "none" && value.parse::<usize>().is_err() {
            eprintln!("jbx: slots takes a number, or `none`");
            return 2;
        }
        if let Err(e) = slots::set_cap(value) {
            eprintln!("jbx: cannot write the cap: {e}");
            return 1;
        }
    }
    let (busy, cap) = slots::busy();
    match cap {
        Some(cap) => jobbox::outln!("  {busy} of {cap} slots busy"),
        None => jobbox::outln!("  {busy} running, no cap (`jbx slots <n>` sets one)"),
    }
    jobbox::outln!("  it holds back `jbx queue` only — a wrapped line is already running.");
    0
}

/// `jbx health` — WHAT RUNS, WHAT IS STUCK, AND WHAT NOBODY WILL READ.
///
/// It answers the question `list` cannot: a job that RUNS is not
/// necessarily a job MAKING PROGRESS, and the two look identical from
/// outside. Freshness of the log separates them.
fn health() -> i32 {
    let records = store::all();
    let mut queued = 0;
    let mut running = 0;
    let mut finished = 0;
    let mut mute: Vec<(String, i64)> = Vec::new();
    for r in &records {
        match store::state_of(r) {
            store::State::Queued => queued += 1,
            store::State::Running { .. } => {
                running += 1;
                if let Some(secs) = store::silence(r) {
                    if secs > store::mute_after() {
                        mute.push((r.id.clone(), secs as i64));
                    }
                }
            }
            _ => finished += 1,
        }
    }
    jobbox::outln!("  {running} running · {queued} queued · {finished} finished and kept");
    let (busy, cap) = jobbox::slots::busy();
    match cap {
        Some(cap) => jobbox::outln!("  {busy} of {cap} slots busy — `jbx queue` waits when they are full"),
        None => jobbox::outln!("  {busy} slots held, no cap"),
    }
    if mute.is_empty() {
        jobbox::outln!("  nothing is mute.");
    } else {
        // NAMED, NOT COUNTED. A number here would send somebody to `list`
        // to find out which one, and the point is to answer that now.
        jobbox::outln!("  MUTE — running, but nothing written to their log for a while:");
        for (id, secs) in &mute {
            jobbox::outln!("    {id}  silent {secs}s   jbx tail {id}");
        }
    }
    let me = store::client();
    let stranded = jobbox::signals::stranded(&me);
    if !stranded.is_empty() {
        jobbox::outln!("  endings addressed to sessions that are gone — nobody will read these:");
        for (who, held) in &stranded {
            jobbox::outln!("    {who}  {held} waiting   jbx signals agent --client {who}");
        }
    }
    if mute.is_empty() && stranded.is_empty() { 0 } else { 1 }
}

/// `jbx clients` — WHOSE ENDINGS ARE STILL UNREAD.
fn clients() -> i32 {
    let me = store::client();
    let all = jobbox::signals::all_clients();
    if all.is_empty() {
        jobbox::outln!("  no mailbox yet — nothing has finished in the background.");
        return 0;
    }
    jobbox::outln!("  {:<28} {:<8} {}", "client", "waiting", "");
    for (who, held) in all {
        let mark = if who == me { "  ← this session" } else { "" };
        jobbox::outln!("  {who:<28} {held:<8}{mark}");
    }
    // THE PERSON'S BOX IS SHARED ON PURPOSE — one human wants every
    // ending, whichever session started it — so it is one line, not a
    // column repeated down the table.
    jobbox::outln!("  {:<28} {:<8}  ← the person, shared by every session",
             "(you)", jobbox::signals::held_for_the_person());
    0
}

/// `jbx config` — EVERY SETTING, AND WHERE IT CAME FROM.
///
/// The point is the SECOND column. A value alone invites the reader to
/// guess whether it is theirs, a project's, or a default — and the day
/// those disagree is the day the question matters.
fn config() -> i32 {
    use jobbox::config;

    let (after, after_from) = config::after();
    let (mute, mute_from) = config::mute_after();
    let (slots, slots_from) = config::slots(jobbox::slots::default_cap());
    let (dir, dir_from) = config::dir(store::root());
    let (compose, compose_from) = config::compose();
    let (on, on_from) = config::enabled();

    let slots = match slots {
        Some(n) if n > 0 => format!("{n} queued jobs at once"),
        Some(_) => "no cap".to_string(),
        None => "no cap".to_string(),
    };

    let rows: Vec<(&str, String, &str)> = vec![
        ("enabled", if on { "yes".into() } else { "NO — jbx stays out of the way here".into() }, on_from.as_str()),
        ("after", format!("{after:.0}s before detaching"), after_from.as_str()),
        ("mute_after", format!("{mute:.0}s of silence is mute"), mute_from.as_str()),
        ("slots", slots, slots_from.as_str()),
        ("dir", dir.display().to_string(), dir_from.as_str()),
        ("integration.rtk.compose", compose.as_str().to_string(), compose_from.as_str()),
    ];
    jobbox::outln!("  {:<24} {:<34} {}", "setting", "value", "from");
    for (name, value, from) in rows {
        jobbox::outln!("  {name:<24} {value:<34} {from}");
    }

    jobbox::outln!();
    jobbox::outln!("  {:<24} {}", "client", store::client());
    let (project, path) = jobbox::stats::project();
    jobbox::outln!("  {:<24} {}  ({path})", "project", project);
    jobbox::outln!("  {:<24} {}", "rtk on the PATH",
                   if which_rtk() { "yes" } else { "no" });

    jobbox::outln!();
    // WHERE TO EDIT, ALWAYS — including when the file is not there. A
    // reader who wants to change something needs the path more than they
    // need to be told the path does not exist yet.
    let global = config::path();
    jobbox::outln!("  global config   {}{}", global.display(),
                   if global.exists() { "" } else { "   (not written yet)" });
    match config::project_path() {
        Some(local) => jobbox::outln!("  this project    {}", local.display()),
        None => jobbox::outln!("  this project    {}/.jbx.yaml   (none — jbx works everywhere by default)",
                               config::project_root().display()),
    }
    0
}

/// Whether rtk is reachable. Asked by running it, not by guessing from a
/// path: what matters is whether it ANSWERS.
fn which_rtk() -> bool {
    std::process::Command::new("rtk")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Whether a word is one of our ids: `j` and seven hex digits, exactly.
///
/// No command looks like this, which is what lets `fg` take either
/// without a flag to disambiguate — and what makes a typo fall through
/// to "run this line" rather than to a wrong job.
fn looks_like_an_id(word: &str) -> bool {
    word.len() == 8
        && word.starts_with('j')
        && word[1..].chars().all(|c| c.is_ascii_hexdigit())
}
