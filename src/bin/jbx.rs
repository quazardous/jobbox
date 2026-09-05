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
        "how" => how(rest.first().filter(|a| looks_like_an_id(a)).map(String::as_str)),
        "describe" => jobbox::describe::describe(),
        "why" => why(),
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
        "stats" => stats::stats(
            rest.first().filter(|a| !a.starts_with('-')).map(String::as_str),
            rest.iter().any(|a| a == "--project-path"),
        ),
        "init" => init::init(rest.iter().any(|a| a == "--undo")),
        "list" => listing(false, &Shape::of(rest)),
        "ps" => listing(true, &Shape::of(rest)),
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
         \x20 jbx ps [--all] [--full] [--json]\n\
         \x20                       what is happening right now, here\n\
         \x20 jbx list              … and what has finished, for a day\n\
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
         \x20 jbx how [id]          what you can do with it, right now\n\
         \x20 jbx why               why it works this way\n\
         \x20 jbx describe          every verb and what it does, as JSON\n\
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
        // TWO WORDS, BECAUSE THEY ARE TWO SITUATIONS. Still held, the
        // output is mirroring to whoever asked and the line may yet
        // finish in time and leave nothing behind; let go of, only the
        // log receives anything and only a verb will bring it back.
        store::State::Running { for_secs, detached: Some(true) } => {
            format!("background {for_secs:.0}s")
        }
        store::State::Running { for_secs, detached: Some(false) } => {
            format!("foreground {for_secs:.0}s")
        }
        // WRITTEN BEFORE THIS TOOL KNEW THE DIFFERENCE. The neutral word
        // is the honest one: saying "foreground" here would assert
        // something nobody observed, which is how a job that had been
        // let go of a quarter of an hour earlier came to read as held.
        store::State::Running { for_secs, detached: None } => {
            format!("running    {for_secs:.0}s")
        }
        store::State::Finished { code } => format!("finished  exit {code}"),
        store::State::Lost => "gone      no exit code".into(),
    }
}

/// `jbx list` — everything kept. `jbx ps` — only what is happening.
///
/// TWO VERBS BECAUSE THEY ANSWER TWO QUESTIONS. "What is going on right
/// now" is asked far more often than "what went on today", and a day of
/// finished jobs between you and the answer is a list you stop reading.
/// How a listing was asked for.
///
/// THE COLUMN CALLED `line` HAS ALWAYS SHOWN THE INTENT — the first four
/// words — which is what makes a list readable three hours later and
/// what makes it useless when two jobs start with the same four words.
/// `--full` is the way back to the line as typed.
pub struct Shape {
    all: bool,
    full: bool,
    json: bool,
}

impl Shape {
    fn of(args: &[String]) -> Shape {
        Shape {
            all: args.iter().any(|a| a == "--all"),
            full: args.iter().any(|a| a == "--full"),
            json: args.iter().any(|a| a == "--json"),
        }
    }
}

fn listing(only_alive: bool, how: &Shape) -> i32 {
    let all = how.all;
    // THIS PROJECT BY DEFAULT. The store is machine-wide, and a list
    // holding four projects' work is a list where you cannot find your
    // own. The scope is the PROJECT and not the session: two Claude
    // Codes open on one directory are working on the same thing, and
    // scoping by session would blind each to half of it.
    let me = jobbox::stats::project().1;
    let alive = |r: &store::Record| {
        matches!(store::state_of(r), store::State::Queued | store::State::Running { .. })
    };
    let everything = store::all();
    let records: Vec<store::Record> = everything
        .iter()
        .filter(|r| (!only_alive || alive(r)) && (all || r.project == me))
        .cloned()
        .collect();
    // WHAT IS HIDDEN AND STILL RUNNING, counted. The tool this replaces
    // showed every session by default, and its reason was good: hiding
    // other people's work makes a full queue look empty, and somebody
    // spends ten minutes wondering why their own job never starts. The
    // default is yours now, so the count is what keeps that honest.
    let others = everything
        .iter()
        .filter(|r| r.project != me && alive(r))
        .count();
    // MACHINE-READABLE, AND EVERYTHING IN IT. A table drops what does not
    // fit a column; this drops nothing, which is the point of asking for
    // it. Empty stays an empty array rather than a sentence.
    if how.json {
        let rows: Vec<serde_json::Value> = records
            .iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "state": describe(&store::state_of(r)).split_whitespace().next().unwrap_or(""),
                    "detached": r.detached,
                    "queued": r.queued,
                    "mirror_cut": r.mirror_cut,
                    "pid": r.pid,
                    "intent": r.intent,
                    "command": r.command,
                    "project": r.project,
                    "client": r.client,
                    "cwd": r.cwd,
                    "started": r.started,
                    "log": store::log_path(&r.id).display().to_string(),
                    "silent_for": store::silence(r),
                })
            })
            .collect();
        jobbox::outln!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
        return 0;
    }
    if records.is_empty() {
        if only_alive {
            jobbox::outln!("nothing running here. `jbx list` shows what has finished.");
        } else {
            jobbox::outln!("nothing detached in this project.");
        }
        if others > 0 && !all {
            jobbox::outln!("{others} running in other projects — `--all` shows them.");
        }
        return 0;
    }
    // THE PROJECT COLUMN ONLY WHEN IT VARIES. Scoped, every row is the
    // same project and the column would be twelve characters of the
    // same word down the page; with `--all` it is the only thing that
    // says which work belongs to what.
    if all {
        jobbox::outln!("{:<10} {:<16} {:<28} {:<12} line", "id", "project", "state", "");
    } else {
        jobbox::outln!("{:<10} {:<28} {:<12} line", "id", "state", "");
    }
    for r in &records {
        // MUTENESS IS ONLY SAID WHEN IT MATTERS. On every line it would
        // be a column people stop reading — and it is precisely the one
        // that must be seen the day it speaks.
        let mute = match store::silence(r) {
            Some(secs) if secs > store::mute_after() => format!("MUTE {}s", secs as i64),
            _ => String::new(),
        };
        if all {
            let project = std::path::Path::new(&r.project)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "?".into());
            jobbox::outln!(
                "{:<10} {:<16} {:<28} {:<12} {}",
                r.id,
                project,
                describe(&store::state_of(r)),
                mute,
                shown_line(r, how)
            );
        } else {
            jobbox::outln!(
                "{:<10} {:<28} {:<12} {}",
                r.id,
                describe(&store::state_of(r)),
                mute,
                shown_line(r, how)
            );
        }
    }
    if others > 0 && !all {
        jobbox::outln!("");
        jobbox::outln!("{others} more running in other projects — `--all` shows them.");
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
    if matches!(state, store::State::Lost) {
        // THE EXPLANATION LIVES HERE, where somebody came to understand
        // one line rather than to scan forty.
        jobbox::outln!("           nothing recorded a code: it was stopped, or the");
        jobbox::outln!("           machine went down under it.");
    }
    jobbox::outln!("  line     {}", r.command);
    jobbox::outln!("  client   {}", r.client);
    jobbox::outln!("  where    {}", r.cwd);
    jobbox::outln!("  log      {}", store::log_path(&r.id).display());
    if r.mirror_cut {
        // THE ONE PLACE THIS CAN BE SAID. Whoever piped the launcher and
        // closed it early had no channel left to be warned on — and the
        // truncated view they kept reads exactly like a finished job.
        jobbox::outln!("  note     whoever was reading the launcher stopped early, so what");
        jobbox::outln!("           they saw was a truncated MIRROR. This log is the whole of it.");
    }
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
        match store::settled_state(&r) {
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
    // WHICH SHELL WILL RUN A LINE. On Windows the answer decides whether
    // anything works at all — the hook quotes for a POSIX shell, so the
    // runner has to be one.
    jobbox::outln!("  {:<24} {}", "shell", match jobbox::run::shell_program() {
        jobbox::run::Shell::Posix(p) => format!("{p} -c"),
        jobbox::run::Shell::Cmd => "cmd /C".into(),
    });
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

/// `jbx why` — THE REASONING, WHERE THE BINARY IS.
///
/// A downloaded binary has no repository to read, and the message that
/// detaches a command has no room to argue. This is where the argument
/// lives: why it wraps everything, why waiting is the cost, and what to
/// do when you genuinely cannot go on without the result.
fn why() -> i32 {
    jobbox::outln!("{}", "\
jbx — why it does that

WHY IT WRAPS EVERY COMMAND
  Because what makes a line slow is not knowable before it runs. A list of
  \"commands worth backgrounding\" is a prediction, and that prediction was
  measured on 136 real calls and refused: no rule at any threshold recovered
  more than 0.7 of the 28 minutes spent waiting, because four of the five
  long shapes had been seen exactly once. By the time a rule knows a command
  is slow, you have already waited through it — and it does not come back.

  So jbx guesses nothing. It runs the line and finds out.

WHY LETTING GO COSTS YOU NOTHING
  Because the ending is ANNOUNCED. A model hears it on a later turn, a person
  when the session stops. Nobody has to remember to look — which is the whole
  reason sitting and waiting is waste: the result comes to you.

  A tool that backgrounded things without telling you would just have moved
  the waiting somewhere you cannot see it.

WHY ASKING FOR THE FOREGROUND IS ALLOWED
  Sometimes you really cannot go on without the answer, and pretending
  otherwise would make the tool something to fight. So saying so is a
  first-class gesture — and it is COUNTED, which is the point. A habit of
  reaching for it becomes visible instead of invisible.

WHY THERE ARE TWO DOORS
  `run` wraps a command that was going to run either way. It holds nothing
  back, so there is nothing to queue and no cap to apply — detaching a line
  does not change how many processes exist.

  `queue <intent> -- '<line>'` takes work that has NOT started. That can wait
  its turn, so `jbx slots` holds it: a loop that files fifty jobs does not
  start fifty at once. It is also the only place a name is required.

WHAT THE NUMBER MEANS
  `jbx stats` says how much time was SAVED — and it means time that ran
  while you were free, not time that vanished. It already subtracts what
  you handed back to `jbx wait`, which is the honest half most tools skip.

  What it cannot see is you waiting some OTHER way. So it is a ceiling,
  made as tight as the evidence allows, and not a receipt.

WHAT IT NEVER DOES
  It never loses an exit code, never holds output back until the end, and
  never breaks a command to save a token. Where there is a terminal, it hands
  the line straight to a shell and stops existing.

  `jbx how` is the other half of this: what to type, rather than why.");
    0
}

/// `jbx how [id]` — WHAT TO DO, RIGHT NOW.
///
/// The other half of `why`. The detachment message used to carry this
/// list, which made it four lines longer than the thing it was trying to
/// say — and the thing it was trying to say is "do not wait". Given an
/// id it answers about that job, so the lines can be copied as they are.
fn how(id: Option<&str>) -> i32 {
    match id {
        Some(id) => {
            jobbox::outln!("jbx: what you can do with {id}, which is running in the background.\n");
            jobbox::outln!("  jbx status {id}   where it is, and its exit code once it lands");
            jobbox::outln!("  jbx tail {id}     what it has printed so far");
            jobbox::outln!("  jbx tail {id} -f  … and keep watching");
            jobbox::outln!("  jbx fg {id}       bring it back to the foreground and watch it");
            jobbox::outln!("  jbx wait {id}     block until it ends — ONLY if you cannot go on");
            jobbox::outln!("  jbx kill {id}     stop it, and everything it started");
            jobbox::outln!("\nBUT THE USUAL ANSWER IS NONE OF THESE. You will be told when it ends,");
            jobbox::outln!("on a later turn. Go and do something else; come back to it then.");
        }
        None => {
            jobbox::outln!("{}", "\
jbx — what to type

NOTHING, USUALLY
  jbx wraps every command your agent runs. Quick ones come back untouched.
  Slow ones let go of themselves and you are told when they end, so the
  ordinary answer to \"what do I do now\" is: something else.

WHEN ONE HAS BEEN DETACHED
  jbx how <id>              this list, for that job
  jbx status <id>           where it is, and its exit code once it lands
  jbx tail <id> [-f]        what it printed
  jbx fg <id>               bring it back and watch it
  jbx wait <id>             block until it ends — only if you cannot go on
  jbx kill <id>             stop it, and everything it started
  jbx list                  everything detached, and how it went

WHEN YOU KNOW IN ADVANCE
  jbx fg -- '<line>'        run it and never let go: you need the answer now
  jbx queue <name> -- '…'   hand work over BEFORE it starts, under a cap
  jbx slots [n|none]        how many queued jobs run at once

TO SEE WHERE THE TIME WENT
  jbx stats [project]       how much of it went by while you were free
  jbx health                what runs, what is mute, what nobody will read

TO SET IT UP
  jbx init [--undo]         declare the hooks; writes a commented config
  jbx config                every setting, and where it came from

  `jbx why` is the other half of this: why it works this way.");
        }
    }
    0
}

/// The intent, or the whole line when it was asked for.
///
/// The intent is four words, which is what makes a list readable and
/// what makes two jobs beginning `cd /long/path && …` indistinguishable.
fn shown_line(r: &store::Record, how: &Shape) -> String {
    if how.full { r.command.replace('\n', " ") } else { r.intent.clone() }
}
