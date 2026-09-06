//! WHAT EACH VERB DOES TO THE WORLD, said by the program itself.
//!
//! A tool that checks an agent's commands before they run needs to tell
//! `jbx list` from `jbx kill`. It can read `--help` for the names, and
//! that is where it goes wrong: the names MOVE — six verbs appeared or
//! changed shape in one evening — and a third party's hardcoded table
//! then classifies an unknown verb at random, inside a program whose
//! whole job is deciding what to let through.
//!
//! So the binary answers for itself. Whatever is installed is right by
//! construction.
//!
//! ────────────────────────────────────────────────────────────────────
//! WHY THE SHAPE IS NOT ENOUGH
//! ────────────────────────────────────────────────────────────────────
//!
//! The document follows OpenCLI — "OpenAPI for command line interfaces"
//! — so the tools that read it can. But a CLI description says a verb
//! `kill` exists and takes an id; it does NOT say that it tears down a
//! process tree. OpenAPI escapes this because HTTP carries the answer in
//! the method: a GET is safe, a POST is not. A command line has no such
//! thing — `jbx list` and `jbx kill` have exactly the same shape.
//!
//! The consequence is what a guard needs, so it is ours to add:
//! `x-jbx-effect`, on every command. That field is the only part of this
//! document we can promise anything about, and the only part worth
//! promising anything about.
//!
//! THE SPECIFICATION IS PRE-1.0 AND CONTESTED — four projects publish an
//! "OpenCLI Specification" under different domains. The document says so
//! itself rather than letting `"opencli"` read as a guarantee.

/// Every verb: its name, what it is for, and WHAT IT DOES TO THE WORLD.
///
/// ONE TABLE, so `--help` and this cannot drift apart. A test compares
/// them in both directions — the README taught that lesson: a guard that
/// only checks for what is false never notices what is missing.
/// THE VOCABULARY, AND IT IS DELIBERATELY SMALL.
///
/// `x-jbx-effect` was a sentence, and a sentence has to be matched as
/// text by whoever reads it — which means guessing, in a program whose
/// job is not guessing. These are simple verbs a guard can compare, and
/// they are declared IN the document so nobody has to know them in
/// advance.
///
/// A verb may carry several: `slots` reads and changes capacity, `fg`
/// runs something and blocks on it.
pub const TAGS: &[(&str, &str)] = &[
    ("read", "observes; changes nothing"),
    ("consume", "reading it destroys it — the same look does not work twice"),
    ("execute", "runs an arbitrary command line"),
    ("create", "makes work that will run later"),
    ("destroy", "stops a running process and everything under it"),
    ("capacity", "changes how much may run from now on"),
    ("configure", "edits settings on this machine, including the harness"),
    ("rewrite", "changes the command the harness is about to run"),
    ("block", "does not return until something else ends"),
];

/// ONE FLAG, WHAT IT DOES.
pub type Flag = (&'static str, &'static str);

/// A verb: what it is for, what it does to the world, and WHAT IT TAKES.
///
/// The flags are here and not in the parser, and that is the point: this
/// table is what the parser accepts, what `--help` prints, and what the
/// document publishes. A flag that exists is documented and a flag that
/// is documented is accepted, because there is only one list.
///
/// Before that there were three, and they drifted the way three lists
/// do: `jbx list --help` printed a table of jobs and said nothing about
/// the flag, and `jbx status --json` was ignored in silence. Neither was
/// a bug in a verb — both were a list nobody had.
pub struct Verb {
    pub name: &'static str,
    pub summary: &'static str,
    pub tags: &'static [&'static str],
    pub effect: &'static str,
    pub flags: &'static [Flag],
}

/// Flags a listing takes, named once because two verbs share them.
const LISTING: &[Flag] = &[
    ("--all", "every project on this machine, not just this one"),
    ("--full", "the line as recorded, wrappers and all, never cut"),
    ("--json", "every field of every record, cut nothing"),
    ("--width", "columns to draw in; `auto` asks the terminal"),
];
const JSON_ONLY: &[Flag] = &[("--json", "answer as JSON rather than as a table")];
const NOTHING: &[Flag] = &[];

/// Every verb. ONE TABLE, so `--help`, the parser and this document
/// cannot drift apart. A test compares them in both directions — the
/// README taught that lesson: a guard that only checks for what is false
/// never notices what is missing.
pub const VERBS: &[Verb] = &[
    Verb { name: "run", summary: "run a line, detaching it if it turns out to be long",
        tags: &["execute"], effect: "runs an arbitrary command line",
        flags: &[("--after", "seconds to hold the caller before detaching"),
                 ("--intent", "what this job is for, in a few words")] },
    Verb { name: "fg", summary: "run a line and never let go, or bring a detached job back",
        tags: &["execute", "block"],
        effect: "runs a line without ever letting go, or attaches to one",
        flags: &[("--intent", "what this job is for, in a few words")] },
    Verb { name: "queue", summary: "hand work over before it starts, under a cap",
        tags: &["create"], effect: "creates pending work", flags: NOTHING },
    Verb { name: "kill", summary: "stop a job and everything it started",
        tags: &["destroy"], effect: "stops a process tree", flags: NOTHING },
    Verb { name: "slots", summary: "how many queued jobs may run at once",
        tags: &["read", "capacity"],
        effect: "changes future capacity; reads when given no number", flags: JSON_ONLY },
    Verb { name: "wait", summary: "block until a job ends, and exit with its code",
        tags: &["read", "block"], effect: "reads, and blocks until the job ends",
        flags: NOTHING },
    Verb { name: "signals", summary: "endings not yet read", tags: &["consume"],
        effect: "reports each ending once and then forgets it",
        flags: &[("--json", "answer as JSON rather than as a table"),
                 ("--client", "read another mailbox than this session's")] },
    Verb { name: "init", summary: "declare the hooks, and take rtk's over",
        tags: &["configure"],
        effect: "edits the harness settings and writes configuration files",
        flags: &[("--undo", "put back what was there before")] },
    Verb { name: "hook", summary: "answer the harness; `init` declares this one",
        tags: &["rewrite"], effect: "rewrites the command the harness is about to run",
        flags: NOTHING },
    Verb { name: "list", summary: "what is detached, and how it went",
        tags: &["read"], effect: "reads", flags: LISTING },
    Verb { name: "ps", summary: "what is happening right now",
        tags: &["read"], effect: "reads", flags: LISTING },
    Verb { name: "status", summary: "one job: where it is, its exit code, its log",
        tags: &["read"], effect: "reads", flags: JSON_ONLY },
    Verb { name: "tail", summary: "what a job printed", tags: &["read"],
        effect: "reads; `-f` blocks until the job ends",
        flags: &[("-f", "keep printing until the job ends")] },
    Verb { name: "stats", summary: "how much time was saved", tags: &["read"],
        effect: "reads",
        flags: &[("--json", "answer as JSON rather than as a table"),
                 ("--project-path", "full paths instead of names"),
                 ("--thresholds", "what another `after` would have cost, replayed")] },
    Verb { name: "health", summary: "what runs, what is mute, what nobody will read",
        tags: &["read"], effect: "reads", flags: JSON_ONLY },
    Verb { name: "clients", summary: "whose endings are still unread",
        tags: &["read"], effect: "reads", flags: JSON_ONLY },
    Verb { name: "config", summary: "every setting, and where it came from",
        tags: &["read"], effect: "reads", flags: JSON_ONLY },
    Verb { name: "describe", summary: "this document", tags: &["read"], effect: "reads",
        flags: NOTHING },
    Verb { name: "how", summary: "what you can do with a job, right now",
        tags: &["read"], effect: "reads", flags: JSON_ONLY },
    Verb { name: "why", summary: "why it works this way", tags: &["read"], effect: "reads",
        flags: JSON_ONLY },
];

/// What one verb takes, for the parser and for `--help`.
pub fn verb(name: &str) -> Option<&'static Verb> {
    VERBS.iter().find(|v| v.name == name)
}

/// `jbx describe` — the document.
pub fn describe() -> i32 {
    let commands: Vec<serde_json::Value> = VERBS
        .iter()
        .map(|v| {
            serde_json::json!({
                "name": v.name,
                "description": v.summary,
                // WHAT IT TAKES, from the same table the parser reads.
                // A flag published here is accepted; one that is not is
                // refused by name. They cannot disagree.
                "options": v.flags
                    .iter()
                    .map(|(flag, what)| serde_json::json!({
                        "name": flag, "description": what,
                    }))
                    .collect::<Vec<_>>(),
                "x-jbx-tags": v.tags,
                "x-jbx-effect": v.effect,
            })
        })
        .collect();

    let document = serde_json::json!({
        "$schema": "https://opencli.org/draft.json",
        "opencli": "0.1",
        "info": {
            "title": "jbx",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Wrap every command line; detach the ones that turn out to be long.",
            "license": { "name": "MIT", "identifier": "MIT" },
        },
        "commands": commands,
        // THE VOCABULARY TRAVELS WITH THE DOCUMENT. A tag a reader has
        // never seen is a tag it would have to guess at, and guessing is
        // what this exists to remove.
        "x-jbx-tag-meanings": TAGS
            .iter()
            .map(|(tag, meaning)| (tag.to_string(), serde_json::Value::from(*meaning)))
            .collect::<serde_json::Map<_, _>>(),
        // SAID IN THE DOCUMENT ITSELF, so that `"opencli": "0.1"` above
        // is not read as a promise: the specification is pre-1.0 and
        // several projects publish one under that name. What we can
        // stand behind is the extension, because it is ours.
        "x-jbx-note": "The OpenCLI specification is pre-1.0 and several projects \
                       publish one; this document follows opencli.org and may \
                       change with it. `x-jbx-effect` is ours: it says what a \
                       verb does to the world, which no CLI schema carries, and \
                       it is the part to rely on. Match on `x-jbx-tags`, which are \
                       simple verbs meant to be compared; `x-jbx-effect` is the \
                       same thing said to a person.",
    });
    outln!("{}", serde_json::to_string_pretty(&document).unwrap_or_default());
    0
}
