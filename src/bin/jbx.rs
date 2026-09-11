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

use jobbox::{default_after, gain, hook, init, run, signals, slots, store, tail};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(dispatch(args));
}

fn dispatch(args: Vec<String>) -> i32 {
    let verb = args.first().map(String::as_str).unwrap_or("");
    let rest = if args.is_empty() { &[][..] } else { &args[1..] };

    match verb {
        "-v" | "-V" | "--version" => {
            jobbox::outln!("jbx {VERSION}");
            0
        }
        // ONE DOOR. `--help`, `help`, and `help <id>` are the same
        // verb: a reader who wants to know anything types one word, and
        // the pages behind it are named there rather than remembered.
        "" | "-h" | "--help" | "help" => with("help", rest, |how| {
            match how.free.first().filter(|a| looks_like_an_id(a)) {
                Some(id) => help_for(id, how),
                // PROSE IS STILL A VALUE, and the rule that every verb
                // declaring `--json` answers JSON has no exception to
                // remember — which is exactly what the test that caught
                // this exists for. `jbx describe` is the structured
                // form of the verb list; this is the page a person
                // reads, wrapped so a machine is never lied to.
                None => Answer(serde_json::json!({ "text": usage() }), 0)
                    .show(how, |v| print!("{}", v["text"].as_str().unwrap_or(""))),
            }
        }),
        "run" => match Flags::of("run", rest) {
            Ok(how) => run::run(
                how.after.unwrap_or_else(default_after),
                &tail(rest),
                how.intent.as_deref(),
                how.no_input,
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
                    // BEFORE THE `--` ONLY: after it is the line, and a line
                    // that happens to say `--no-input` is not asking for it.
                    rest.iter().take_while(|a| a.as_str() != "--").any(|a| a == "--no-input"),
                )
            }
            None => 2,
        },
        "hook" => {
            let binary = std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "jbx".into());
            // NAMED, OR CLAUDE. The name is what a settings file spells
            // out, so an unknown one is REFUSED LOUDLY rather than
            // treated as Claude: a hook that quietly speaks the wrong
            // dialect answers nothing and looks perfectly healthy, which
            // is the failure this whole table exists to avoid.
            // `--list` BELONGS ON THE VERB THAT CONSUMES THE TABLE, not
            // on a verb of its own: the question "which names does --cli
            // take" is asked from here, and `jbx clients` already means
            // something else entirely — whose endings are still unread.
            //
            // ITS BODY LIVES OUTSIDE THIS MATCH, and that is not taste: a
            // guard reads these arms to check that every verb answered is
            // a declared one, and it took the string literals of an
            // inlined body for verbs. A thin arm is what makes it legible.
            if rest.iter().any(|a| a == "--list") {
                return with("hook", rest, list_dialects);
            }
            match dialect_named(rest.first().map(String::as_str)) {
                Some(d) => hook::hook(&binary, d),
                None => 2,
            }
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
                Some(first) if looks_like_an_id(first) && rest.len() == 1 => {
                    note_grip(first, "brought back");
                    run::attach(first)
                }
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
        "after" => with("after", rest, |f| after_cmd(f.free.first().map(String::as_str), f)),

        "describe" => with("describe", rest, |_| jobbox::describe::describe()),
        "how" => with("how", rest, how_to),
        "why" => with("why", rest, why),
        "bench" => with("bench", rest, |how| jobbox::bench::bench(
            how.free.first().and_then(|n| n.parse().ok()).unwrap_or(60),
            how.json,
        )),
        "health" => with("health", rest, health),
        "clients" => with("clients", rest, clients),
        "config" => with("config", rest, config),
        "signals" => with("signals", rest, |how| match how.free.first() {
            Some(audience) => signals::signals(audience, how.json, how.client.as_deref()),
            None => usage_error("signals needs an audience: agent or user"),
        }),
        "gain" => with("gain", rest, |how| if how.reset {
            gain_reset(how)
        } else { match gain::measure(
            how.free.first().map(String::as_str),
            how.since,
        ) {
            Err(code) => code,
            Ok(v) => Answer(v, 0).show(how, |v| {
                gain::render(v, how.project_path, how.thresholds)
            }),
        } }),
        "init" => with("init", rest, |how| match dialect_named(how.cli.as_deref()) {
            Some(d) => init::init(how.undo, how.global_only, how.core, how.announce, d),
            None => 2,
        }),
        "watch" => with("watch", rest, |how| jobbox::watch::watch(how.all, how.json)),
        "list" => with("list", rest, |how| listing(false, how)),
        "ps" => with("ps", rest, |how| listing(true, how)),
        "top" => with("top", rest, top),
        "prune" => with("prune", rest, prune),
        "status" => with("status", rest, |how| match how.free.first() {
            Some(id) => status(id, how),
            None => usage_error("status needs an id"),
        }),
        "tail" => with("tail", rest, |how| match how.free.first() {
            Some(id) => {
                note_grip(id, "read");
                tail_log(id, how.follow)
            }
            None => usage_error("tail needs an id"),
        }),
        "wait" => with("wait", rest, |how| match how.free.first() {
            Some(id) if how.via_agent && !jobbox::config::allow_wait().0 => refuse_wait(id),
            Some(id) => wait(id),
            None => usage_error("wait needs an id"),
        }),
        "kill" => with("kill", rest, |how| match (how.free.first(), how.older_than) {
            (Some(id), _) => {
                note_grip(id, "killed");
                kill(id)
            }
            (None, Some(age)) => kill_older_than(age, how.all),
            (None, None) => usage_error("kill needs an id, or `--too-old`"),
        }),
        // A FLAG IS NOT A VERB, and saying so is the difference between
        // "you spelled the verb wrong" and "that flag goes after one".
        // Somebody typing `jbx -x` was told they had invented a verb.
        other if other.starts_with('-') => {
            eprintln!("jbx: {other:?} is a flag, and jbx wants a verb first — `jbx <verb> {other}`");
            eprint!("{}", usage());
            2
        }
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
         \x20 jbx hook [client]     answer an agent CLI — claude, gemini\n\
         \n\
         \x20 jbx ps [--all] [--full] [--json] [--width <n>]\n\
         \x20                       what is happening right now, here\n\
         \x20 jbx top [--all]       the same, redrawn, until you stop it\n\
         \x20 jbx prune [--all]     forget what is over, and what cannot be true\n\
         \x20 jbx list              … and what has finished, for a day\n\
         \x20 jbx watch             one line per job event, until nothing runs\n\
         \x20 jbx status <id>       state, exit code, where its log is\n\
         \x20 jbx tail <id> [-f]    what it printed\n\
         \x20 jbx wait <id>         block until it ends, exit with its code\n\
         \x20 jbx kill <id> | --too-old | --older-than <age>\n\
         \x20                       stop it, and everything it started\n\
         \x20 jbx slots [n|none]    how many queued jobs may run at once\n\
         \x20 jbx after [seconds]   how long a line may hold before detaching\n\
         \x20 jbx signals <who>     endings not yet read: agent or user\n\
         \x20 jbx gain [project]   what the wrapping bought, and cost\n\
         \x20 jbx bench [runs]      what the wrapping costs, per command\n\
         \x20 jbx health            what runs, what is mute, what is stranded\n\
         \x20 jbx clients           whose endings are still unread\n\
         \x20 jbx config            every setting, and where it came from\n\
         \x20 jbx help [id]         this, or what to do with one job\n\
         \x20 jbx how               the gestures: what to do, and when\n\
         \x20 jbx why               why it works this way\n\
         \x20 jbx describe          every verb and what it does, as JSON\n\
         \x20 jbx init [--undo] [--global-only] [--core|--announce]\n\
         \x20                       declare the wrapping hook, displacing rtk's\n\
         \n\
         JBX_AFTER   seconds before detaching (now {:.0})\n\
         JBX_DIR          where logs and records live (now {})\n\
         \n\
         A job that just detached: `jbx help <id>`. What to do, and when:\n\
         `jbx how`. Why it is built this way at all: `jbx why`.\n",
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
        // `held` NEVER REACHES HERE — the state alone cannot tell it, so
        // `held_or` below asks the record. Kept as a note because the
        // next person will look for it in this match first.
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

/// THE STATE, WITH WHAT ONLY THE RECORD KNOWS.
///
/// `describe` reads a state, and a state cannot say whether anybody ever
/// intended to let this job go. A line the harness is already running in
/// the background is held on purpose and reads as `foreground` — which
/// is true, and which somebody scanning a listing takes for "blocked for
/// thirty-five minutes".
fn held_or(r: &store::Record, state: &store::State) -> String {
    match state {
        store::State::Running { for_secs, detached: Some(false) } if r.held => {
            format!("held       {for_secs:.0}s")
        }
        other => describe(other),
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
    global_only: bool,
    core: bool,
    announce: bool,
    list: bool,
    cli: Option<String>,
    follow: bool,
    /// `jbx gain --reset`: forget the readings instead of reading them.
    reset: bool,
    /// Seconds past which a still-running job is stopped by `prune`.
    older_than: Option<f64>,
    /// Written by the hook onto a `jbx wait` the AGENT typed, never onto
    /// one the harness backgrounds for it. See `allow_wait`.
    via_agent: bool,
    project_path: bool,
    thresholds: bool,
    since: Option<f64>,
    width: Option<usize>,
    client: Option<String>,
    after: Option<f64>,
    intent: Option<String>,
    /// Written by the hook onto every line it rewrites: the line gets no
    /// standard input. See `run::supervise`.
    no_input: bool,
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
                "--via-agent" => flags.via_agent = true,
                "--reset" => flags.reset = true,
                // AN HOUR, SPELLED AS A WORD because that is the case
                // people arrive with: something has been sitting there
                // far too long and they want it gone.
                "--too-old" => flags.older_than = Some(3600.0),
                "--older-than" => match value().as_deref().map(str::trim).map(parse_age) {
                    Some(Some(secs)) => flags.older_than = Some(secs),
                    _ => return Err(usage_error(
                        "`--older-than` wants an age: 30s, 45m, 2h — a bare number means minutes",
                    )),
                },
                "--undo" => flags.undo = true,
                "--global-only" => flags.global_only = true,
                "--core" => flags.core = true,
                "--announce" => flags.announce = true,
                "--list" => flags.list = true,
                "--cli" => match value() {
                    Some(name) => flags.cli = Some(name),
                    None => {
                        eprintln!("jbx: --cli names a client: {}",
                            jobbox::dialect::DIALECTS.iter().map(|d| d.name)
                                .collect::<Vec<_>>().join(", "));
                        return Err(2);
                    }
                },
                "--project-path" => flags.project_path = true,
                "--thresholds" => flags.thresholds = true,
                "--since" => match value().as_deref().map(jobbox::gain::window) {
                    Some(Some(span)) => flags.since = span,
                    _ => {
                        return Err(usage_error(
                            "`--since` wants a span like `1h`, `24h`, `7d`, or `all`",
                        ))
                    }
                },
                "-f" => flags.follow = true,
                "--client" => flags.client = value(),
                "--intent" => flags.intent = value(),
                "--no-input" => flags.no_input = true,
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
    }
    for (flag, what) in v.flags {
        text.push_str(&format!("  {flag:<16} {what}\n"));
    }
    // AFTER THE FLAGS, because a flag is what you came for and a note is
    // what you did not know you needed.
    text.push_str(v.notes);
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

/// AN AGE, WRITTEN THE WAY PEOPLE WRITE ONE.
///
/// A bare number means minutes, because that is the unit somebody has in
/// mind when they say a job has been there too long. Seconds and hours
/// are spelled out.
fn parse_age(text: &str) -> Option<f64> {
    let (number, scale) = match text.chars().last()? {
        's' => (&text[..text.len() - 1], 1.0),
        'm' => (&text[..text.len() - 1], 60.0),
        'h' => (&text[..text.len() - 1], 3600.0),
        _ => (text, 60.0),
    };
    let n: f64 = number.trim().parse().ok()?;
    (n > 0.0).then_some(n * scale)
}

/// `jbx gain --reset [--all]` — SAY WHAT WAS FORGOTTEN, CHECKABLY.
///
/// A PROJECT NAMED ALONGSIDE IT IS REFUSED rather than ignored. `jbx gain
/// other --reset` reads as "reset `other`", and quietly resetting this
/// project instead is how somebody erases the history they meant to keep.
fn gain_reset(how: &Flags) -> i32 {
    if let Some(named) = how.free.first() {
        return usage_error(&format!(
            "`--reset` forgets this project, or every one with `--all` — not `{named}`. \
             Run it from that project's directory."
        ));
    }
    let (gone, kept, span) = gain::reset(how.all);
    if gone == 0 {
        jobbox::outln!("nothing to forget — no readings here.");
        return 0;
    }
    let day = |t: f64| {
        let secs = t as i64;
        let days = secs.div_euclid(86400);
        // Civil date from days since the epoch — no dependency for one line.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        format!("{d:02}/{m:02}")
    };
    let whose = if how.all { "every project".to_string() } else { jobbox::gain::project().0 };
    let when = span
        .map(|(a, b)| format!(" from {} to {}", day(a), day(b)))
        .unwrap_or_default();
    let readings = |n: usize| if n == 1 { "1 reading".to_string() } else { format!("{n} readings") };
    jobbox::outln!("forgot {} for {whose}{when}.", readings(gone));
    if kept > 0 {
        let were = if kept == 1 { "was" } else { "were" };
        jobbox::outln!("{}", jobbox::paint::dim(&format!(
            "{} from other projects {were} kept.", readings(kept)
        )));
    }
    0
}

/// `jbx prune` — FORGET WHAT IS OVER, AND WHAT CANNOT BE TRUE.
///
/// Two kinds of record deserve removing, and NOTHING ELSE DOES.
///
///   FINISHED — it has an exit code. `list` keeps a day of these on
///   purpose, and this is the way to say you have read them.
///
///   SICK — the record says a job is running and no process answers to
///   its pid. Nobody is waiting on it, nothing will ever write its code,
///   and it will sit in every listing looking like work in progress
///   until the six-hour sweep gets to it.
///
/// IT STOPS NOTHING. A job whose process is alive is left exactly where
/// it is, however old and however quiet — because age is not a fault and
/// silence is not either. The thirty-five-minute job that prompted this
/// verb was a harness's own background loop, held on purpose and mute by
/// design, and a prune that killed by age would have killed it first.
///
/// EACH REMOVAL IS NAMED AS IT HAPPENS. A destructive verb that prints a
/// count has told you nothing you can check.
fn prune(how: &Flags) -> i32 {
    let me = jobbox::gain::project().1;
    let mut finished = 0;
    let mut sick = 0;
    for r in store::all() {
        if !how.all && r.project != me {
            continue;
        }
        // SETTLED, because `Lost` is the answer a race produces out of
        // nothing — a supervisor between its last write and its exit is
        // momentarily neither running nor recorded. Removing a live
        // job's record on the strength of one glance is the one mistake
        // this verb must not make.
        let why = match store::settled_state(&r) {
            store::State::Finished { code } => Some(format!("finished {code}")),
            store::State::Lost => Some("gone — no process, and no exit code".into()),
            _ => None,
        };
        let Some(why) = why else { continue };
        if store::forget(&r.id) {
            if why.starts_with("finished") { finished += 1 } else { sick += 1 }
            // THE LINE ITSELF, TRIMMED — an id and a verdict tell you a
                // record went, not which one. Somebody scanning this wants
                // to recognise their own command.
                let line: String = r.command.chars().take(46).collect();
                jobbox::outln!("  {} {:<34} {}", r.id, why, jobbox::paint::dim(&line));
        }
    }
    if finished + sick == 0 {
        jobbox::outln!("nothing to forget — everything here is still happening.");
        return 0;
    }
    jobbox::outln!();
    jobbox::outln!("{}", jobbox::paint::dim(&format!(
        "{finished} finished, {sick} that could not be true. Anything still running was left alone."
    )));
    0
}

/// `jbx top` — `jbx ps`, REDRAWN, FOR SOMEBODY WATCHING.
///
/// The same table as `ps`, and deliberately the same code drawing it: a
/// second renderer would drift from the first, and the day it did the
/// live view would be the one nobody trusts.
///
/// IT DOES NOT TAKE THE ALTERNATE SCREEN, and that is a constraint
/// rather than a preference. Restoring it on Ctrl-C needs a signal
/// handler, this project carries no dependency for one, and a `top`
/// that leaves the alternate buffer up has broken the terminal of
/// whoever just wanted to look. Clearing the normal buffer costs a
/// leftover frame on exit — which is what `watch(1)` leaves too, and
/// nobody has ever minded.
///
/// WITHOUT A TERMINAL IT IS ONE SNAPSHOT. There is nothing to redraw
/// into, and a loop that never ends is how a pipe becomes a hang.
fn top(how: &Flags) -> i32 {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return listing(true, how);
    }
    loop {
        // HOME, THEN ERASE WHAT IS BELOW — not a full clear, which
        // blanks the screen for one frame and reads as a flicker.
        print!("\x1b[H\x1b[J");
        let code = listing(true, how);
        if code != 0 {
            return code;
        }
        jobbox::outln!("\n\x1b[2mrefreshing every second — Ctrl-C to stop\x1b[0m");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn listing(only_alive: bool, how: &Flags) -> i32 {
    // SOMEBODY IS LOOKING, so this is when the drawer gets tidied. See
    // `signals::sweep` for why here and not in a hook, and why never in
    // `run`: those wrap every command on the machine.
    signals::sweep();
    let all = how.all;
    // THIS PROJECT BY DEFAULT. The store is machine-wide, and a list
    // holding four projects' work is a list where you cannot find your
    // own. The scope is the PROJECT and not the session: two Claude
    // Codes open on one directory are working on the same thing, and
    // scoping by session would blind each to half of it.
    let me = jobbox::gain::project().1;
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
                    // SAID AS ITS OWN FIELD rather than folded into the
                    // state word: a reader filtering on `state` should
                    // not have to learn a new value, and one that cares
                    // about deliberate holds can ask for this.
                    "held": r.held,
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
        // AND NEVER ON A HELD JOB. The harness runs those in the
        // background itself and they print nothing for minutes at a
        // time by design — an `until` loop has nothing to say until it
        // is over. Calling that mute is raising an alarm about a
        // silence somebody chose.
        let mute = match store::silence(r) {
            Some(secs) if secs > store::mute_after() && !r.held => {
                format!("MUTE {}s", secs as i64)
            }
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
                held_or(r, &store::state_of(r)),
                mute,
                cell(given_name(r)),
                shown_line(r, how, wide)
            );
        } else {
            jobbox::outln!(
                "{:<10} {:>5} {:<16} {:<10} {}{}",
                r.id,
                age(r),
                held_or(r, &store::state_of(r)),
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
    // SETTLED, BECAUSE A SCRIPT ACTS ON THIS ANSWER. `ps` and `list` can
    // read `gone` for a job whose supervisor is between its last write
    // and its exit — the next refresh corrects them. A single-shot
    // answer has no next refresh.
    let state = store::settled_state(&r);
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
                gain::record_wait(store::now() - began);
                // THE ENDING HAS NOW BEEN DELIVERED, so the message
                // announcing it has no recipient left. Waiting IS the
                // delivery: this call blocked until the job ended and is
                // about to exit with its code.
                //
                // IT MATTERS MOST WHERE NO HOOK REPORTS ENDINGS. The
                // announcing hooks used to empty this box every turn; an
                // install that declares only the wrapping hook has
                // nothing that does, so an unread ending would sit there
                // until the session died and then be listed by `jbx
                // health` as stranded, for ever, one box per session — an
                // alarm that always rings and is therefore never read.
                signals::forget(&store::client(), id);
                return code;
            }
            store::State::Lost => {
                gain::record_wait(store::now() - began);
                // A JOB THAT LOST ITS EXIT CODE STILL ENDED, and this
                // call still carried that news. The box is cleared for
                // the same reason.
                signals::forget(&store::client(), id);
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
/// `jbx kill --too-old` — STOP WHAT HAS BEEN GOING ON TOO LONG.
///
/// The id form asks about one job somebody has looked at. This one is
/// for the other situation: a listing full of things that should have
/// ended, and no wish to name them one at a time.
///
/// EACH ONE IS NAMED AS IT GOES. Stopping another session's work
/// silently is not a tidy-up, and a count tells you nothing you could
/// have checked. Held jobs are not spared: a harness's background loop
/// still waiting after an hour is exactly what somebody reaching for
/// this is looking at.
///
/// AND IT DOES NOT FORGET THEM. The record stays, so the log stays with
/// it — a job you have just stopped is the one whose output you are
/// most likely to want. `jbx prune` clears them afterwards.
fn kill_older_than(age: f64, all: bool) -> i32 {
    let me = jobbox::gain::project().1;
    // NEVER THE HAND THAT IS DOING THIS. `jbx kill` is itself a wrapped
    // command, so its own wrapper has a record — and with a short age it
    // is old enough to match. Stopping it kills the kill, half way
    // through, and the caller sees a command that died for no reason it
    // can name.
    let mine = jobbox::store::ancestors();
    let mut stopped = 0;
    for r in store::all() {
        if !all && r.project != me {
            continue;
        }
        let ran_for = store::now() - r.started;
        if ran_for < age || !store::alive(r.pid) || mine.contains(&r.pid) {
            continue;
        }
        note_grip(&r.id, "killed");
        jobbox::outln!("  {} running for {:.0}m — stopping it", r.id, ran_for / 60.0);
        kill(&r.id);
        stopped += 1;
    }
    if stopped == 0 {
        jobbox::outln!("nothing has been running that long.");
    } else {
        jobbox::outln!();
        jobbox::outln!("{}", jobbox::paint::dim(&format!(
            "{stopped} stopped. Their logs are still here — `jbx prune` clears the records."
        )));
    }
    0
}

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
/// WHERE A SETTING GOES WHEN SOMEBODY SETS ONE.
///
/// THE PROJECT'S FILE WHEN THERE IS A PROJECT. Slots and the threshold
/// are the two things worth changing per repository — a deployment tree
/// wants a different cut from a crate that builds in four seconds — and
/// the project file is where the model already says such a thing lives.
///
/// ONLY WHERE A PROJECT ACTUALLY BEGINS, though. `project_root` falls
/// back to the working directory when it finds no marker, and a
/// `.jbx.yaml` dropped into whatever directory somebody was standing in
/// is litter, not configuration — the same rule `init` follows. Failing
/// a project, the global file, and the answer says which was written.
fn set_setting(key: &str, value: &str) -> i32 {
    let root = jobbox::config::project_root();
    let project = root.join(".claude").exists() || root.join(".git").exists();
    let file = if project { root.join(".jbx.yaml") } else { jobbox::config::path() };
    match jobbox::config::set_in(&file, key, value) {
        Ok(()) => {
            jobbox::outln!("  {key}: {value}");
            jobbox::outln!("  written to {}", file.display());
            if !project {
                jobbox::outln!("  (no project here, so this is the global file)");
            }
            0
        }
        Err(e) => {
            eprintln!("jbx: cannot write {}: {e}", file.display());
            1
        }
    }
}

/// `jbx after [seconds]` — READ IT, OR SET IT FOR THIS PROJECT.
fn after_cmd(value: Option<&str>, how: &Flags) -> i32 {
    if let Some(value) = value {
        match value.trim().parse::<f64>() {
            Ok(n) if n >= 0.0 => return set_setting("after", &format!("{n:.0}")),
            _ => {
                eprintln!("jbx: after takes a number of seconds");
                return 2;
            }
        }
    }
    let (secs, from) = jobbox::config::after();
    Answer(serde_json::json!({ "after": secs, "from": from.as_str() }), 0).show(how, |v| {
        jobbox::outln!("  {:.0}s before a long line detaches itself", v["after"].as_f64().unwrap_or(0.0));
        jobbox::outln!("  from {}", text(v, "from"));
        jobbox::outln!("  `jbx after <seconds>` sets it for this project.");
    })
}

fn slots_cmd(value: Option<&str>, how: &Flags) -> i32 {
    if let Some(value) = value {
        if value != "none" && value.parse::<usize>().is_err() {
            eprintln!("jbx: slots takes a number, or `none`");
            return 2;
        }
        return set_setting("slots", if value == "none" { "0" } else { value });
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
    // BEFORE COUNTING, TIDY — otherwise this verb reports a backlog it
    // was about to clear, which is how a list nobody can act on grows.
    signals::sweep();
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

/// SAY WHAT TO DO INSTEAD, and DO NOT NAME THE SETTING.
///
/// A refusal that only refuses sends the caller looking for another way
/// to stand still, and there is always one — a `sleep` in a loop, a
/// `tail -f`, a poll every second. So this names the gesture that was
/// wanted.
///
/// It does NOT name `allow_wait`. Telling an agent which setting forbade
/// this is telling it where to go and switch the guardrail off, and a
/// resourceful one will: the config file is a file, and editing files is
/// what it does all day. The person who set it already knows it exists —
/// `jbx config` says so, to whoever asks.
fn refuse_wait(id: &str) -> i32 {
    eprintln!(
        "jbx: this waiting is Monitor's, not yours.\n\
         \x20 Hand `jbx wait {id}` to Monitor — it ends when the job does, and that\n\
         \x20 ending wakes you. Then carry on with this turn.\n\
         \x20 jbx help {id}    everything else you can do with it"
    );
    2
}

fn config(how: &Flags) -> i32 {
    use jobbox::config;

    let (after, after_from) = config::after();
    let (mute, mute_from) = config::mute_after();
    let (slots, slots_from) = config::slots(jobbox::slots::default_cap());
    let (dir, dir_from) = config::dir(store::root());
    let (compose, compose_from) = config::compose();
    let (on, on_from) = config::enabled();
    let (waiting, waiting_from) = config::allow_wait();
    let (width, width_from) = config::width();
    let (color, color_from) = config::color();

    let slots_said = match slots {
        Some(n) if n > 0 => format!("{n} queued jobs at once"),
        _ => "no cap".to_string(),
    };
    let (project, path) = jobbox::gain::project();
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
                row("allow_wait",
                    if waiting { "the agent may run `jbx wait` itself" }
                    else { "`jbx wait` is Monitor's; the agent's own is refused" }.to_string(),
                    waiting.into(), waiting_from.as_str()),
                row("mute_after", format!("{mute:.0}s of silence is mute"), mute.into(),
                    mute_from.as_str()),
                row("slots", slots_said, slots.into(), slots_from.as_str()),
                row("width",
                    width.map(|w| format!("{w} columns")).unwrap_or_else(|| "auto".into()),
                    width.into(), width_from.as_str()),
                row("color",
                    match color {
                        Some(true) => "always".into(),
                        Some(false) => "never".into(),
                        None => format!("auto — {} here", if jobbox::paint::wanted() {
                            "on"
                        } else {
                            "off, nothing is reading this as a terminal"
                        }),
                    },
                    color.into(), color_from.as_str()),
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
/// NOTE A GESTURE THAT ONLY A NAME MADE POSSIBLE.
///
/// Only for a job that actually LET GO. Reaching for one that never
/// detached is reaching for something the caller was standing over
/// anyway — it needed no name and proves nothing about wrapping.
///
/// Silent when the id is unknown: this is bookkeeping beside the verb,
/// and a failed lookup is the verb's business to report, not ours.
/// A CLIENT BY NAME, OR CLAUDE — and an unknown name is refused loudly.
///
/// A settings file spells the name out, so a typo must not be read as
/// "the default": a hook speaking the wrong dialect answers nothing and
/// looks perfectly healthy, which is the failure this table exists for.
/// EVERY CLIENT THIS BINARY CAN ANSWER, read off the same table the hook
/// consumes — so a name printed here is a name that works.
fn list_dialects(how: &Flags) -> i32 {
    let this_binary = std::env::current_exe().ok().and_then(|p| std::fs::canonicalize(p).ok());
    let this_version = env!("CARGO_PKG_VERSION");
    let rows: Vec<serde_json::Value> = jobbox::dialect::DIALECTS
        .iter()
        .map(|d| {
            serde_json::json!({
                "name": d.name,
                "tool": d.tool,
                "before_tool": d.before_tool,
                "settings": d.home_dir,
                "reports_unasked": d.turn_end.is_some(),
                "hook": declared_hook(d, this_binary.as_deref(), this_version),
            })
        })
        .collect();
    Answer(serde_json::Value::Array(rows), 0).show(how, |v| {
        for d in v.as_array().into_iter().flatten() {
            let quiet = d["reports_unasked"] != true;
            jobbox::outln!(
                "{:<8} {:<20} {:<12} {}{}",
                d["name"].as_str().unwrap_or(""),
                d["tool"].as_str().unwrap_or(""),
                d["before_tool"].as_str().unwrap_or(""),
                match d["settings"].as_str().unwrap_or("") {
                    // NAMED AS A LIMIT, NOT LEFT BLANK. `jbx hook <name>`
                    // answers these; only `jbx init` cannot declare them,
                    // because each keeps its hooks in a file of another
                    // shape. A blank column reads as "nothing to say".
                    "" => "declare by hand".to_string(),
                    dir => format!("~/{dir}/settings.json"),
                },
                if quiet { "  (no unasked endings)" } else { "" },
            );
            let h = &d["hook"];
            let at = |key: &str| h[key].as_str().unwrap_or("?").to_string();
            let said = if h["checkable"] != true {
                "not checkable here: jbx does not know where this client keeps its hooks".to_string()
            } else if h["declared"] != true {
                format!("not declared in {}", at("file"))
            } else if h["exists"] != true {
                format!("declared, but {} does not exist", at("binary"))
            } else if h["version"].is_null() {
                format!("declared, but {} did not answer --version within 3s", at("binary"))
            } else {
                let mut s = format!("declared: {} answers jbx {}", at("binary"), at("version"));
                if h["is_this_binary"] != true {
                    s.push_str(", not this binary");
                }
                if h["same_version"] != true {
                    s.push_str(&format!(" — this one is {}", at("this_version")));
                }
                s
            };
            jobbox::outln!("         {said}");
        }
    })
}

/// WHAT IS REALLY DECLARED FOR ONE CLIENT, and whether it answers.
///
/// The listing used to name the file a hook belongs in and say nothing of
/// what was in it. On 10/09 the hook pointed at a jbx two versions behind
/// the published one, and nothing said so: "gain looks broken" found it.
/// So this reads the file, follows the declared command to its binary, and
/// asks that binary its version. It never writes.
fn declared_hook(
    d: &jobbox::dialect::Dialect,
    this_binary: Option<&std::path::Path>,
    this_version: &str,
) -> serde_json::Value {
    let Some((file, commands)) = init::declared_hooks(d) else {
        return serde_json::json!({ "checkable": false });
    };
    let Some(command) = commands.first() else {
        return serde_json::json!({
            "checkable": true, "file": file.display().to_string(), "declared": false,
        });
    };
    let binary = command.split_whitespace().next().unwrap_or("");
    let exists = std::path::Path::new(binary).is_file();
    let version = if exists { version_of(binary) } else { None };
    let is_this = this_binary.is_some()
        && std::fs::canonicalize(binary).ok().as_deref() == this_binary;
    serde_json::json!({
        "checkable": true,
        "file": file.display().to_string(),
        "declared": true,
        "declarations": commands.len(),
        "command": command,
        "binary": binary,
        "exists": exists,
        "version": version,
        "is_this_binary": is_this,
        "this_version": this_version,
        "same_version": version.as_deref() == Some(this_version),
    })
}

/// THE VERSION A DECLARED BINARY ANSWERS, or nothing within three seconds.
///
/// A half-written binary once froze every command of a session. The one
/// being listed may be exactly that, and the listing must not freeze with
/// it: past the deadline it is stopped, and said not to have answered.
fn version_of(binary: &str) -> Option<String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    // `jbx 0.18.0` — the second word is the version.
    out.split_whitespace().nth(1).map(str::to_string)
}

fn dialect_named(want: Option<&str>) -> Option<&'static jobbox::dialect::Dialect> {
    let want = want.unwrap_or("claude");
    let found = jobbox::dialect::of(want);
    if found.is_none() {
        let known: Vec<&str> = jobbox::dialect::DIALECTS.iter().map(|d| d.name).collect();
        eprintln!("jbx: no dialect for {want:?} — known: {}", known.join(", "));
    }
    found
}

fn note_grip(id: &str, verb: &str) {
    // `detached` IS THREE-VALUED, and the third value matters: a
    // record written before the field existed says None, never false.
    // Treating that as "did not detach" would assert something nobody
    // observed, so it simply does not count.
    if store::read_record(id).and_then(|r| r.detached) == Some(true) {
        jobbox::gain::record_touch(id, verb);
    }
}

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
/// `jbx how` — THE GESTURES, in the order you meet them.
///
/// The third door. `help` is the map, `why` is the reasoning, and this
/// is what to actually DO — which used to live scattered through `why`,
/// where someone looking for an instruction had to read an argument to
/// find it.
fn how_to(how: &Flags) -> i32 {
    let text = "\
jbx — how to work with it

A JOB JUST DETACHED
  Do not wait for it. Go do the next thing; its ending is announced and
  will reach you on a later turn without you looking.

      jbx help <id>            what to do with THAT job, lines to copy

WHEN THERE IS GENUINELY NOTHING ELSE TO DO
  Do not poll — polling is waiting with extra steps, and it spends a turn
  per look. Hand the waiting to whatever runs your commands:

      jbx wait <id>            as a BACKGROUND command, not a foreground
                               one. It exits when the job does and carries
                               its exit code, so its ending wakes you.
      jbx watch --json         one line per job event, for a monitor. It
                               ENDS BY ITSELF when nothing is left running.

  Cover the failures too. A watch that speaks only on success is silent
  through a crash, and silence looks exactly like \"still running\".

WHEN YOU WANT TO STOP IT
  Detaching is what gave the line a NAME, and the name is the only way to
  reach it while it runs. A foreground line has none: once it starts you
  are committed to whatever it does, up to its timeout.

      jbx list                 what is running, and under which id
      jbx kill <id>            stop it, AND everything it started

  That last part is not decoration. A shell line is normally one process
  that spawns others; signalling the parent alone leaves the real work
  running under a new one.

WHEN YOU REALLY CANNOT GO ON WITHOUT THE RESULT
  Say so, rather than fighting the tool:

      jbx fg -- '<line>'       never lets go
      jbx fg <id>              bring a detached job back to the front

  It is counted, so the habit stays visible. `jbx gain` says what it cost.

WHEN THE WORK HAS NOT STARTED YET
  A line already running cannot be held back. Work you are ABOUT to file
  can be — that is the other door, and the only one that takes a name:

      jbx queue '<intent>' -- '<line>'
      jbx slots 4              how many queued jobs may run at once

READING WHAT HAPPENED
      jbx ps                   what is running, here
      jbx status <id>          state, exit code, where its log is
      jbx tail <id> [-f]       what it printed
      jbx gain                what takes time — a ceiling, not a receipt

  `jbx help` is the map. `jbx why` is the reasoning behind all of this.";
    Answer(serde_json::json!({ "text": text }), 0)
        .show(how, |v| jobbox::outln!("{}", v["text"].as_str().unwrap_or("")))
}

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
  `jbx gain` says how much time was SAVED — and it means time that ran
  while you were free, not time that vanished. It already subtracts what
  you handed back to `jbx wait`, which is the honest half most tools skip.

  What it cannot see is you waiting some OTHER way. So it is a ceiling,
  made as tight as the evidence allows, and not a receipt.

WHAT IT NEVER DOES
  It never loses an exit code, never holds output back until the end, and
  never breaks a command to save a token. Where there is a terminal, it hands
  the line straight to a shell and stops existing.

WHY LETTING GO IS ALSO WHAT GIVES YOU A GRIP
  Detaching is usually described as time handed back. It is also the moment
  the line acquires a NAME — and the name is the only way to reach it while
  it still runs. A foreground line has none: once it starts you are committed
  to everything it does, right up to its timeout, including the part you
  would have stopped had you known.

  Measured on 08/09/2026: a three-step line detached at sixty seconds; the
  freed turn was spent reading a document that said the third step would
  create something billable and unusable. It was killed between the second
  step and the third. Nothing had judged that line long — that is the point.

WHY WAITING IS THE THING TO AVOID
  Because the result comes to you. Standing over a detached job buys nothing
  that the announcement does not already give you, and it costs the whole
  duration twice: your attention, and the tokens of a session doing nothing.

  Polling is the same waiting in other clothes, and it spends a turn per
  look. There is a way to hand the waiting over instead — `jbx how` says
  which, since that is a gesture and this page is only the reasoning.

  `jbx help` is the map, `jbx how` is the gestures, and this is the argument
  behind both.";
    // PROSE IS STILL A VALUE. One field rather than none, so that the
    // rule "every verb answers something a machine can read" has no
    // exceptions to remember.
    Answer(serde_json::json!({ "text": text }), 0)
        .show(how, |v| jobbox::outln!("{}", v["text"].as_str().unwrap_or("")))
}

/// `jbx help <id>` — WHAT TO DO WITH THIS JOB, RIGHT NOW.
///
/// The other half of `why`. The detachment message used to carry this
/// list, which made it four lines longer than the thing it was trying to
/// say — and the thing it was trying to say is "do not wait". Given an
/// id it answers about that job, so the lines can be copied as they are.
fn help_for(id: &str, flags: &Flags) -> i32 {
    // AND IT HAS TO EXIST. "What you can do with j1234567, which is
    // running in the background" was printed for any eight characters
    // shaped like an id — a sentence stating as fact something nobody
    // had looked up.
    let Some(record) = store::read_record(id) else {
        eprintln!("jbx: {id} is unknown — `jbx list` says what there is.");
        return 1;
    };
    let state = describe(&store::state_of(&record));
    // A MACHINE WANTS THE COMMANDS, A PERSON WANTS THE SENTENCE.
    // Both are built from this one list, so neither can go stale
    // while the other is updated.
    let offers: Vec<(String, &str)> = vec![
        (format!("jbx status {id}"), "where it is, and its exit code once it lands"),
        (format!("jbx tail {id}"), "what it has printed so far"),
        (format!("jbx tail {id} -f"), "… and keep watching"),
        (format!("jbx fg {id}"), "bring it back to the foreground and watch it"),
        (format!("jbx wait {id}"),
         "give this to Monitor: it ends when the job does, and that wakes you"),
        (format!("jbx kill {id}"), "stop it, and everything it started"),
    ];
Answer(
        serde_json::json!({
            "id": id,
            "state": state,
            "commands": offers.iter().map(|(c, w)| serde_json::json!({
                "command": c, "what": w,
            })).collect::<Vec<_>>(),
            "advice": "You will be told when it ends, on a later turn. \
                       Go and do something else; come back to it then.",
        }),
        0,
    )
    .show(flags, |v| {
        jobbox::outln!("jbx: {id} — {}.\n", text(v, "state"));
        for c in v["commands"].as_array().map(Vec::as_slice).unwrap_or_default() {
            jobbox::outln!("  {:<22} {}", text(c, "command"), text(c, "what"));
        }
        jobbox::outln!("\nTHE USUAL ANSWER IS NONE OF THESE. You will be told when it ends, on a");
        jobbox::outln!("later turn — go and do something else. With nothing else to do, hand");
        jobbox::outln!("`jbx wait` to Monitor rather than polling, or running it in front of");
        jobbox::outln!("you: it ends when the job does, so the ending wakes you.");
})
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
/// dropped here is reading room, and `gain` must still group on what ran.
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
    cut(&jobbox::gain::without_preamble(&r.command).replace('\n', " "), width)
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

