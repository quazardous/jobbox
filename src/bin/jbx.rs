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
        "run" => match Flags::of("run", rest) {
            Ok(how) => run::run(
                how.after.unwrap_or_else(default_after),
                &tail(rest),
                how.intent.as_deref(),
            ),
            Err(code) => code,
        },
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
        "fg" => match Flags::of("fg", rest) {
            Err(code) => code,
            Ok(how) => match rest.first() {
                Some(first) if looks_like_an_id(first) && rest.len() == 1 => run::attach(first),
                Some(_) => run::foreground(&tail(rest), how.intent.as_deref()),
                None => usage_error("fg needs a line or a job id"),
            },
        },
        "queue" => match rest.first() {
            Some(intent) if intent != "--" && rest.len() > 1 => {
                run::queue(intent, &tail(&rest[1..]))
            }
            _ => usage_error("queue needs an intent and a line: `jbx queue build -- make`"),
        },
        "slots" => with("slots", rest, |f| slots_cmd(f.free.first().map(String::as_str), f)),
        "how" => with("how", rest, |f| {
            how(f.free.first().filter(|a| looks_like_an_id(a)).map(String::as_str), f)
        }),
        "describe" => with("describe", rest, |_| jobbox::describe::describe()),
        "why" => with("why", rest, why),
        "health" => with("health", rest, health),
        "clients" => with("clients", rest, clients),
        "config" => with("config", rest, config),
        "signals" => with("signals", rest, |how| match how.free.first() {
            Some(audience) => signals::signals(audience, how.json, how.client.as_deref()),
            None => usage_error("signals needs an audience: agent or user"),
        }),
        "stats" => with("stats", rest, |how| match stats::measure(
            how.free.first().map(String::as_str),
        ) {
            Err(code) => code,
            Ok(v) => Answer(v, 0).show(how, |v| {
                stats::render(v, how.project_path, how.thresholds)
            }),
        }),
        "init" => with("init", rest, |how| init::init(how.undo)),
        "list" => with("list", rest, |how| listing(false, how)),
        "ps" => with("ps", rest, |how| listing(true, how)),
        "status" => with("status", rest, |how| match how.free.first() {
            Some(id) => status(id, how),
            None => usage_error("status needs an id"),
        }),
        "tail" => with("tail", rest, |how| match how.free.first() {
            Some(id) => tail_log(id, how.follow),
            None => usage_error("tail needs an id"),
        }),
        "wait" => with("wait", rest, |how| match how.free.first() {
            Some(id) => wait(id),
            None => usage_error("wait needs an id"),
        }),
        "kill" => with("kill", rest, |how| match how.free.first() {
            Some(id) => kill(id),
            None => usage_error("kill needs an id"),
        }),
        other => {
            eprintln!("jbx: unknown verb {other:?}");
            eprint!("{}", usage());
            2
        }
    }
}

/// PARSE, THEN DO — and if the parsing failed, that IS the answer.
///
/// Every verb goes through here, so no verb can forget to refuse a flag
/// it does not take, and none has to remember to handle `--help`.
fn with(verb: &str, args: &[String], go: impl FnOnce(&Flags) -> i32) -> i32 {
    match Flags::of(verb, args) {
        Ok(how) => go(&how),
        Err(code) => code,
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
         \x20 jbx ps [--all] [--full] [--json] [--width <n>]\n\
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
        store::State::Queued => "queued".into(),
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
        store::State::Lost => "gone".into(),
    }
}

/// `jbx list` — everything kept. `jbx ps` — only what is happening.
///
/// TWO VERBS BECAUSE THEY ANSWER TWO QUESTIONS. "What is going on right
/// now" is asked far more often than "what went on today", and a day of
/// finished jobs between you and the answer is a list you stop reading.
/// WHAT A VERB ANSWERED, BEFORE ANYBODY DECIDED HOW TO SHOW IT.
///
/// A verb builds a value; this decides whether it is printed as JSON or
/// rendered for a person — and the rendering reads THE SAME VALUE, so
/// the table and the JSON cannot say different things. Written by hand
/// side by side, they did: `--json` existed on three verbs out of twenty
/// because each one had to be remembered separately.
struct Answer(serde_json::Value, i32);

impl Answer {
    fn show(self, how: &Flags, human: impl FnOnce(&serde_json::Value)) -> i32 {
        if how.json {
            jobbox::outln!("{}", serde_json::to_string_pretty(&self.0).unwrap_or_default());
        } else {
            human(&self.0);
        }
        self.1
    }
}

/// A `&str` out of a value, because every reader below wants one and
/// `as_str().unwrap_or("")` twenty times reads like an accident.
fn text<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

/// EVERY FLAG OF EVERY VERB, PARSED IN ONE PLACE.
///
/// What a verb accepts is declared in `describe::VERBS` — the same table
/// `jbx describe` publishes — so a flag cannot be accepted without being
/// documented, nor documented without being accepted.
///
/// AN UNKNOWN FLAG IS AN ERROR, NOT A NO-OP. `jbx list --help` printed a
/// table of jobs and said nothing about the flag; `jbx status --json`
/// was ignored in silence. Neither was a bug in a verb: both were a typo
/// that looked like it worked, which is the one failure this program
/// refuses everywhere else.
#[derive(Default)]
pub struct Flags {
    all: bool,
    full: bool,
    json: bool,
    undo: bool,
    follow: bool,
    project_path: bool,
    thresholds: bool,
    width: Option<usize>,
    client: Option<String>,
    after: Option<f64>,
    intent: Option<String>,
    /// Positional arguments in order — an id, a project name, a number.
    free: Vec<String>,
}

impl Flags {
    fn of(verb: &str, args: &[String]) -> Result<Flags, i32> {
        let known = jobbox::describe::verb(verb).map(|v| v.flags).unwrap_or(&[]);
        let mut flags = Flags::default();
        let mut rest = args.iter();
        while let Some(arg) = rest.next() {
            // EVERYTHING AFTER A BARE `--` IS THE COMMAND LINE. It is
            // not ours to read, and reading it is how a wrapper starts
            // altering the thing it wraps.
            if arg == "--" {
                break;
            }
            if !arg.starts_with('-') {
                flags.free.push(arg.clone());
                continue;
            }
            let (flag, inline) = match arg.split_once('=') {
                Some((flag, value)) => (flag, Some(value.to_string())),
                None => (arg.as_str(), None),
            };
            if flag == "-h" || flag == "--help" {
                print!("{}", verb_usage(verb));
                return Err(0);
            }
            if !known.iter().any(|(name, _)| *name == flag) {
                eprintln!("jbx: `{flag}` is not a flag `jbx {verb}` takes");
                eprint!("{}", verb_usage(verb));
                return Err(2);
            }
            let mut value = || inline.clone().or_else(|| rest.next().cloned());
            match flag {
                "--all" => flags.all = true,
                "--full" => flags.full = true,
                "--json" => flags.json = true,
                "--undo" => flags.undo = true,
                "--project-path" => flags.project_path = true,
                "--thresholds" => flags.thresholds = true,
                "-f" => flags.follow = true,
                "--client" => flags.client = value(),
                "--intent" => flags.intent = value(),
                "--after" => match value().as_deref().map(str::trim).map(str::parse::<f64>) {
                    Some(Ok(n)) => flags.after = Some(n),
                    _ => return Err(usage_error("`--after` wants a number of seconds")),
                },
                "--width" => match value().as_deref().map(str::trim) {
                    Some("auto") => flags.width = None,
                    Some(n) => match n.parse::<usize>() {
                        Ok(n) => flags.width = Some(n.max(40)),
                        Err(_) => return Err(usage_error("`--width` wants columns, or `auto`")),
                    },
                    None => return Err(usage_error("`--width` wants columns, or `auto`")),
                },
                // UNREACHABLE BY CONSTRUCTION: the table said it exists.
                // If this ever fires, the table gained a flag nobody
                // taught the parser, and saying so beats ignoring it.
                other => {
                    eprintln!("jbx: `{other}` is declared but not implemented — please report it");
                    return Err(70);
                }
            }
        }
        Ok(flags)
    }
}

/// What ONE verb takes, printed from the table that accepts it.
fn verb_usage(name: &str) -> String {
    let Some(v) = jobbox::describe::verb(name) else {
        return usage();
    };
    let mut text = format!("jbx {} — {}\n", v.name, v.summary);
    if v.flags.is_empty() {
        text.push_str("  takes no flags.\n");
        return text;
    }
    for (flag, what) in v.flags {
        text.push_str(&format!("  {flag:<16} {what}\n"));
    }
    text
}

/// How wide the table may draw itself.
///
/// The flag first, then the setting, then the terminal — and 100 when
/// nothing can say, which is the case that matters most: the usual
/// reader of `jbx ps` is an agent with no terminal at all.
fn table_width(how: &Flags) -> usize {
    if let Some(asked) = how.width {
        return asked;
    }
    if let (Some(set), _) = jobbox::config::width() {
        return set;
    }
    terminal_columns().unwrap_or(100)
}

/// THE TERMINAL IS ASKED, NOT GUESSED — and asked through a program
/// rather than through an `unsafe` call into libc, which is the same
/// call this project already declined to make for `/proc` on Windows.
///
/// `COLUMNS` is not consulted: a shell keeps it as its own variable and
/// does not export it, so reading it here answers for whoever last
/// exported one by hand — an old width, confidently wrong.
///
/// stdin is the controlling terminal and not the inherited one, so a
/// listing still measures right with something piped into it. No
/// terminal at all — a harness, `/dev/tty` returning ENXIO — is a `None`
/// and a fallback, never a guess.
fn terminal_columns() -> Option<usize> {
    use std::process::{Command, Stdio};
    let tty = std::fs::File::open("/dev/tty").ok()?;
    let out = Command::new("stty")
        .arg("size")
        .stdin(Stdio::from(tty))
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).split_whitespace().nth(1)?.parse().ok()
}

fn listing(only_alive: bool, how: &Flags) -> i32 {
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
    // WHAT IS LEFT, SHARED BETWEEN THE TWO COLUMNS THAT HOLD TEXT. The
    // others are as wide as what they hold and no wider; these two take
    // the room the terminal gives, because a full-screen window cutting
    // a line at 46 characters is the tool wasting what it was given.
    let fixed = 10 + 1 + 5 + 1 + if all { 14 + 1 } else { 0 } + 16 + 1 + 10 + 1;
    let free = table_width(how).saturating_sub(fixed).max(20);
    // AND THE NAME COLUMN ONLY WHEN SOMEBODY NAMED SOMETHING. A derived
    // name is the first four words of the line printed beside the line —
    // thirty columns that repeat what is already there. It appears when
    // a caller actually said what the work was for, and then it is the
    // most useful thing on the row.
    // AND NOT IN A NARROW WINDOW EITHER: below about forty-five columns
    // of free space the two of them would each be too short to read, and
    // the line is the one that cannot be guessed from anything else.
    let name_width = if free >= 45 && records.iter().any(|r| !given_name(r).is_empty()) {
        (free * 2 / 5).clamp(20, 60)
    } else {
        0
    };
    let cell = |text: &str| -> String {
        if name_width == 0 {
            String::new()
        } else {
            format!("{:<name_width$} ", cut(text, name_width))
        }
    };
    // AND THE SPACE AFTER IT BELONGS TO IT. The cell prints
    // `<name><space>`, so leaving that space out of the share made the
    // row one character wider than the terminal — which is invisible
    // until a full-screen window wraps every line of the table.
    let wide = free - name_width - usize::from(name_width > 0);
    if all {
        jobbox::outln!(
            "{:<10} {:>5} {:<14} {:<16} {:<10} {}line",
            "id",
            "age",
            "project",
            "state",
            "",
            cell("intent")
        );
    } else {
        jobbox::outln!(
            "{:<10} {:>5} {:<16} {:<10} {}line",
            "id", "age", "state", "", cell("intent")
        );
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
                "{:<10} {:>5} {:<14} {:<16} {:<10} {}{}",
                r.id,
                age(r),
                project,
                describe(&store::state_of(r)),
                mute,
                cell(given_name(r)),
                shown_line(r, how, wide)
            );
        } else {
            jobbox::outln!(
                "{:<10} {:>5} {:<16} {:<10} {}{}",
                r.id,
                age(r),
                describe(&store::state_of(r)),
                mute,
                cell(given_name(r)),
                shown_line(r, how, wide)
            );
        }
    }
    if others > 0 && !all {
        jobbox::outln!("");
        jobbox::outln!("{others} more running in other projects — `--all` shows them.");
    }
    0
}

fn status(id: &str, how: &Flags) -> i32 {
    let Some(r) = store::read_record(id) else {
        eprintln!("jbx: {id} is unknown");
        return 1;
    };
    let state = store::state_of(&r);
    // THE JOB'S CODE BECOMES OURS, so a script can decide without
    // reading a word of this.
    let code = match state {
        store::State::Finished { code: 0 } => 0,
        store::State::Finished { .. } => 1,
        _ => 0,
    };
    Answer(
        serde_json::json!({
            "id": r.id,
            "state": describe(&state),
            "queued": matches!(state, store::State::Queued),
            "lost": matches!(state, store::State::Lost),
            "exit": match state { store::State::Finished { code } => Some(code), _ => None },
            "detached": r.detached,
            "pid": r.pid,
            "intent": r.intent,
            "line": r.command,
            "client": r.client,
            "cwd": r.cwd,
            "project": r.project,
            "started": r.started,
            "silent_for": store::silence(&r),
            "mirror_cut": r.mirror_cut,
            "log": store::log_path(&r.id).display().to_string(),
        }),
        code,
    )
    .show(how, |v| {
        jobbox::outln!("  id       {}", text(v, "id"));
        jobbox::outln!("  state    {}", text(v, "state"));
        if v["queued"] == true {
            // "QUEUED IS NOT STUCK." It is waiting its turn, and saying
            // which is the difference between leaving it alone and going
            // to look for a fault that is not there.
            jobbox::outln!("           waiting for a slot — nothing is wrong; `jbx slots`");
            jobbox::outln!("           says how many may run at once.");
        }
        if v["lost"] == true {
            // THE EXPLANATION LIVES HERE, where somebody came to
            // understand one line rather than to scan forty.
            jobbox::outln!("           nothing recorded a code: it was stopped, or the");
            jobbox::outln!("           machine went down under it.");
        }
        jobbox::outln!("  line     {}", text(v, "line"));
        jobbox::outln!("  client   {}", text(v, "client"));
        jobbox::outln!("  where    {}", text(v, "cwd"));
        jobbox::outln!("  log      {}", text(v, "log"));
        if v["mirror_cut"] == true {
            // THE ONE PLACE THIS CAN BE SAID. Whoever piped the launcher
            // and closed it early had no channel left to be warned on —
            // and the truncated view they kept reads exactly like a
            // finished job.
            jobbox::outln!("  note     whoever was reading the launcher stopped early, so what");
            jobbox::outln!("           they saw was a truncated MIRROR. This log is the whole of it.");
        }
    })
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
        jobbox::outln!("jbx: {id} is unknown");
        return 1;
    };
    stop(r.pid, "TERM");
    // ASKED, THEN CHECKED. `kill` reporting success does not mean the
    // process went — and on one CI runner the group form failed while
    // reporting nothing useful, leaving a job that read as waiting for a
    // slot it had been stopped from ever taking. What is reported here
    // is what was observed, not what was attempted.
    for _ in 0..20 {
        if !store::alive(r.pid) {
            // SAID OUT LOUD, because the code will not say it: a killed
            // line leaves an interrupt code a later reader would take
            // for a failure of the command itself.
            jobbox::outln!("jbx: {id} stopped");
            return 0;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // IT DID NOT GO. Ask harder rather than report a stop that did not
    // happen — and then check again, for the same reason as before.
    stop(r.pid, "KILL");
    for _ in 0..20 {
        if !store::alive(r.pid) {
            jobbox::outln!("jbx: {id} stopped (it needed KILL)");
            return 0;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    jobbox::outln!("jbx: {id} is still there (pid {}) — nothing this tool can do", r.pid);
    1
}

/// Signal a job: the whole tree, then the supervisor itself.
///
/// BOTH, because neither is enough alone. The group carries everything
/// the line started, which is the point; but the group form is written
/// `-PID`, which some `kill` implementations read as an option, and one
/// runner refused it. Naming the process too costs one more call and
/// removes the dependency on that form working everywhere.
#[cfg(unix)]
fn stop(pid: u32, signal: &str) {
    let dash = format!("-{signal}");
    let group = format!("-{pid}");
    // `--` ENDS THE OPTIONS, so a negative pid cannot be read as one.
    let _ = std::process::Command::new("kill")
        .args([dash.as_str(), "--", group.as_str()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = std::process::Command::new("kill")
        .args([dash.as_str(), "--", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(windows)]
fn stop(pid: u32, signal: &str) {
    // Windows has one hammer; `/T` takes the tree with it.
    let _ = signal;
    let _ = std::process::Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// `jbx slots [n]` — READ THE CAP, OR SET IT.
///
/// It governs `queue` alone. A wrapped line is already running by the
/// time this tool sees it, so capping those would cap nothing — the
/// message says so rather than letting a number imply otherwise.
fn slots_cmd(value: Option<&str>, how: &Flags) -> i32 {
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
    Answer(serde_json::json!({ "busy": busy, "cap": cap }), 0).show(how, |v| {
        match v["cap"].as_u64() {
            Some(cap) => jobbox::outln!("  {} of {cap} slots busy", v["busy"]),
            None => jobbox::outln!("  {} running, no cap (`jbx slots <n>` sets one)", v["busy"]),
        }
        jobbox::outln!("  it holds back `jbx queue` only — a wrapped line is already running.");
    })
}

fn health(how: &Flags) -> i32 {
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
    let (busy, cap) = jobbox::slots::busy();
    let stranded = jobbox::signals::stranded(&store::client());
    let code = if mute.is_empty() && stranded.is_empty() { 0 } else { 1 };
    Answer(
        serde_json::json!({
            "running": running,
            "queued": queued,
            "finished": finished,
            "slots_busy": busy,
            "slots_cap": cap,
            "mute": mute.iter().map(|(id, secs)| serde_json::json!({
                "id": id, "silent_for": secs,
            })).collect::<Vec<_>>(),
            "stranded": stranded.iter().map(|(who, held)| serde_json::json!({
                "client": who, "waiting": held,
            })).collect::<Vec<_>>(),
        }),
        code,
    )
    .show(how, |v| {
        jobbox::outln!(
            "  {} running · {} queued · {} finished and kept",
            v["running"], v["queued"], v["finished"]
        );
        match v["slots_cap"].as_u64() {
            Some(cap) => jobbox::outln!(
                "  {} of {cap} slots busy — `jbx queue` waits when they are full", v["slots_busy"]
            ),
            None => jobbox::outln!("  {} slots held, no cap", v["slots_busy"]),
        }
        let mute = v["mute"].as_array().map(Vec::as_slice).unwrap_or_default();
        if mute.is_empty() {
            jobbox::outln!("  nothing is mute.");
        } else {
            // NAMED, NOT COUNTED. A number here would send somebody to
            // `list` to find out which one, and the point is to answer
            // that now.
            jobbox::outln!("  MUTE — running, but nothing written to their log for a while:");
            for m in mute {
                let id = text(m, "id");
                jobbox::outln!("    {id}  silent {}s   jbx tail {id}", m["silent_for"]);
            }
        }
        let stranded = v["stranded"].as_array().map(Vec::as_slice).unwrap_or_default();
        if !stranded.is_empty() {
            jobbox::outln!("  endings addressed to sessions that are gone — nobody will read these:");
            for s in stranded {
                let who = text(s, "client");
                jobbox::outln!("    {who}  {} waiting   jbx signals agent --client {who}", s["waiting"]);
            }
        }
    })
}

fn clients(how: &Flags) -> i32 {
    let me = store::client();
    let all = jobbox::signals::all_clients();
    Answer(
        serde_json::json!({
            "me": me,
            "clients": all.iter().map(|(who, held)| serde_json::json!({
                "client": who, "waiting": held, "is_this_session": *who == me,
            })).collect::<Vec<_>>(),
            // THE PERSON'S BOX IS SHARED ON PURPOSE — one human wants
            // every ending, whichever session started it — so it is one
            // field, not a column repeated down the table.
            "person_waiting": jobbox::signals::held_for_the_person(),
        }),
        0,
    )
    .show(how, |v| {
        let rows = v["clients"].as_array().map(Vec::as_slice).unwrap_or_default();
        if rows.is_empty() {
            jobbox::outln!("  no mailbox yet — nothing has finished in the background.");
            return;
        }
        jobbox::outln!("  {:<28} {:<8} {}", "client", "waiting", "");
        for row in rows {
            let mark = if row["is_this_session"] == true { "  ← this session" } else { "" };
            jobbox::outln!("  {:<28} {:<8}{mark}", text(row, "client"), row["waiting"]);
        }
        jobbox::outln!("  {:<28} {:<8}  ← the person, shared by every session",
                 "(you)", v["person_waiting"]);
    })
}

fn config(how: &Flags) -> i32 {
    use jobbox::config;

    let (after, after_from) = config::after();
    let (mute, mute_from) = config::mute_after();
    let (slots, slots_from) = config::slots(jobbox::slots::default_cap());
    let (dir, dir_from) = config::dir(store::root());
    let (compose, compose_from) = config::compose();
    let (on, on_from) = config::enabled();
    let (width, width_from) = config::width();

    let slots_said = match slots {
        Some(n) if n > 0 => format!("{n} queued jobs at once"),
        _ => "no cap".to_string(),
    };
    let (project, path) = jobbox::stats::project();
    let shell = match jobbox::run::shell_program() {
        jobbox::run::Shell::Posix(p) => format!("{p} -c"),
        jobbox::run::Shell::Cmd => "cmd /C".into(),
    };
    let global = config::path();

    // ONE ROW PER SETTING, AND IT CARRIES ITS OWN VALUE. The table used
    // to hold a sentence — "30s before detaching" — and nothing else, so
    // reading a setting back meant parsing English.
    let row = |name: &str, said: String, value: serde_json::Value, from: &str| {
        serde_json::json!({ "setting": name, "said": said, "value": value, "from": from })
    };
    Answer(
        serde_json::json!({
            "settings": [
                row("enabled",
                    if on { "yes".into() } else { "NO — jbx stays out of the way here".to_string() },
                    on.into(), on_from.as_str()),
                row("after", format!("{after:.0}s before detaching"), after.into(),
                    after_from.as_str()),
                row("mute_after", format!("{mute:.0}s of silence is mute"), mute.into(),
                    mute_from.as_str()),
                row("slots", slots_said, slots.into(), slots_from.as_str()),
                row("width",
                    width.map(|w| format!("{w} columns")).unwrap_or_else(|| "auto".into()),
                    width.into(), width_from.as_str()),
                row("dir", dir.display().to_string(), dir.display().to_string().into(),
                    dir_from.as_str()),
                row("integration.rtk.compose", compose.as_str().to_string(),
                    compose.as_str().into(), compose_from.as_str()),
            ],
            "client": store::client(),
            "project": project,
            "project_path": path,
            "shell": shell,
            "rtk_on_the_path": which_rtk(),
            "global_config": global.display().to_string(),
            "global_config_exists": global.exists(),
            "project_config": config::project_path().map(|p| p.display().to_string()),
            "project_config_would_be":
                format!("{}/.jbx.yaml", config::project_root().display()),
        }),
        0,
    )
    .show(how, |v| {
        jobbox::outln!("  {:<24} {:<34} {}", "setting", "value", "from");
        for r in v["settings"].as_array().map(Vec::as_slice).unwrap_or_default() {
            jobbox::outln!("  {:<24} {:<34} {}",
                           text(r, "setting"), text(r, "said"), text(r, "from"));
        }
        jobbox::outln!();
        jobbox::outln!("  {:<24} {}", "client", text(v, "client"));
        jobbox::outln!("  {:<24} {}  ({})", "project", text(v, "project"), text(v, "project_path"));
        // WHICH SHELL WILL RUN A LINE. On Windows the answer decides
        // whether anything works at all — the hook quotes for a POSIX
        // shell, so the runner has to be one.
        jobbox::outln!("  {:<24} {}", "shell", text(v, "shell"));
        jobbox::outln!("  {:<24} {}", "rtk on the PATH",
                       if v["rtk_on_the_path"] == true { "yes" } else { "no" });
        jobbox::outln!();
        // WHERE TO EDIT, ALWAYS — including when the file is not there.
        // A reader who wants to change something needs the path more
        // than they need to be told the path does not exist yet.
        jobbox::outln!("  global config   {}{}", text(v, "global_config"),
                       if v["global_config_exists"] == true { "" } else { "   (not written yet)" });
        match v["project_config"].as_str() {
            Some(local) => jobbox::outln!("  this project    {local}"),
            None => jobbox::outln!("  this project    {}   (none — jbx works everywhere by default)",
                                   text(v, "project_config_would_be")),
        }
    })
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
fn why(how: &Flags) -> i32 {
    let text = "\
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

  `jbx how` is the other half of this: what to type, rather than why.";
    // PROSE IS STILL A VALUE. One field rather than none, so that the
    // rule "every verb answers something a machine can read" has no
    // exceptions to remember.
    Answer(serde_json::json!({ "text": text }), 0)
        .show(how, |v| jobbox::outln!("{}", v["text"].as_str().unwrap_or("")))
}

/// `jbx how [id]` — WHAT TO DO, RIGHT NOW.
///
/// The other half of `why`. The detachment message used to carry this
/// list, which made it four lines longer than the thing it was trying to
/// say — and the thing it was trying to say is "do not wait". Given an
/// id it answers about that job, so the lines can be copied as they are.
fn how(id: Option<&str>, flags: &Flags) -> i32 {
    if let Some(id) = id {
        // A MACHINE WANTS THE COMMANDS, A PERSON WANTS THE SENTENCE.
        // Both are built from this one list, so neither can go stale
        // while the other is updated.
        let offers: Vec<(String, &str)> = vec![
            (format!("jbx status {id}"), "where it is, and its exit code once it lands"),
            (format!("jbx tail {id}"), "what it has printed so far"),
            (format!("jbx tail {id} -f"), "… and keep watching"),
            (format!("jbx fg {id}"), "bring it back to the foreground and watch it"),
            (format!("jbx wait {id}"), "block until it ends — ONLY if you cannot go on"),
            (format!("jbx kill {id}"), "stop it, and everything it started"),
        ];
        return Answer(
            serde_json::json!({
                "id": id,
                "commands": offers.iter().map(|(c, w)| serde_json::json!({
                    "command": c, "what": w,
                })).collect::<Vec<_>>(),
                "advice": "You will be told when it ends, on a later turn. \
                           Go and do something else; come back to it then.",
            }),
            0,
        )
        .show(flags, |v| {
            jobbox::outln!("jbx: what you can do with {id}, which is running in the background.\n");
            for c in v["commands"].as_array().map(Vec::as_slice).unwrap_or_default() {
                jobbox::outln!("  {:<18} {}", text(c, "command"), text(c, "what"));
            }
            jobbox::outln!("\nBUT THE USUAL ANSWER IS NONE OF THESE. You will be told when it ends,");
            jobbox::outln!("on a later turn. Go and do something else; come back to it then.");
        });
    }
    let text = "\
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

  `jbx why` is the other half of this: why it works this way.";
    Answer(serde_json::json!({ "text": text }), 0)
        .show(flags, |v| jobbox::outln!("{}", v["text"].as_str().unwrap_or("")))
}

/// The line itself, beside the intent rather than instead of it.
///
/// BOTH COLUMNS, BECAUSE THEY ANSWER DIFFERENT QUESTIONS: the intent
/// says what somebody meant to do, the line says what is actually
/// running, and showing one of them left the other to guesswork.
///
/// `--full` prints the line as recorded. Without it the wrappers go —
/// `cd <root> &&`, `timeout <n>`, `rtk proxy` — because that is forty
/// characters of identical preamble standing where the difference
/// between two jobs should be. THE FINGERPRINT KEEPS THEM: what is
/// dropped here is reading room, and stats must still group on what ran.
/// HOW LONG AGO IT STARTED, which is the one instant every record
/// holds. A finished job otherwise carried no time at all: `finished
/// exit 0` reads the same for something that ended a minute ago and
/// something that ended yesterday, and a list is read to tell them
/// apart.
///
/// RELATIVE, NOT A CLOCK. A clock time is a timezone, and this program
/// carries no calendar to be right about one — `--json` publishes the
/// instant itself, which is where an exact answer belongs.
fn age(r: &store::Record) -> String {
    let secs = (store::now() - r.started).max(0.0);
    match secs {
        s if s < 90.0 => format!("{s:.0}s"),
        s if s < 5400.0 => format!("{:.0}m", s / 60.0),
        s if s < 172_800.0 => format!("{:.0}h", s / 3600.0),
        s => format!("{:.0}d", s / 86400.0),
    }
}

/// The name a CALLER gave, and nothing when the name was read off the
/// line: `store::read_record` derives that one, so equality with what it
/// would derive is exactly the question "did anybody say?".
fn given_name(r: &store::Record) -> &str {
    if r.intent == store::intent_of(&r.command) {
        ""
    } else {
        &r.intent
    }
}

fn shown_line(r: &store::Record, how: &Flags, width: usize) -> String {
    if how.full {
        return r.command.replace('\n', " ");
    }
    cut(&jobbox::stats::without_preamble(&r.command).replace('\n', " "), width)
}

/// Shorten to a column, and SAY SO with an ellipsis rather than stopping
/// mid-word as though that were the whole of it. `--full` and `--json`
/// are where the untruncated line lives.
fn cut(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars().take(width - 1).collect::<String>() + "…"
}

