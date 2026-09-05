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

use crate::outln;

/// Every verb: its name, what it is for, and WHAT IT DOES TO THE WORLD.
///
/// ONE TABLE, so `--help` and this cannot drift apart. A test compares
/// them in both directions — the README taught that lesson: a guard that
/// only checks for what is false never notices what is missing.
pub const VERBS: &[(&str, &str, &str)] = &[
    ("run", "run a line, detaching it if it turns out to be long",
     "runs an arbitrary command line"),
    ("fg", "run a line and never let go, or bring a detached job back",
     "runs an arbitrary command line"),
    ("queue", "hand work over before it starts, under a cap",
     "creates pending work"),
    ("kill", "stop a job and everything it started",
     "stops a process tree"),
    ("slots", "how many queued jobs may run at once",
     "changes future capacity; reads when given no number"),
    ("wait", "block until a job ends, and exit with its code",
     "reads, and blocks until the job ends"),
    ("signals", "endings not yet read",
     "CONSUMES: an ending is reported once and then gone"),
    ("init", "declare the hooks, and take rtk's over",
     "edits the harness settings and writes configuration files"),
    ("hook", "answer the harness; `init` declares this one",
     "rewrites the command the harness is about to run"),
    ("list", "what is detached, and how it went", "reads"),
    ("ps", "what is happening right now", "reads"),
    ("status", "one job: where it is, its exit code, its log", "reads"),
    ("tail", "what a job printed", "reads"),
    ("stats", "how much time was saved", "reads"),
    ("health", "what runs, what is mute, what nobody will read", "reads"),
    ("clients", "whose endings are still unread", "reads"),
    ("config", "every setting, and where it came from", "reads"),
    ("describe", "this document", "reads"),
    ("how", "what you can do with a job, right now", "reads"),
    ("why", "why it works this way", "reads"),
];

/// `jbx describe` — the document.
pub fn describe() -> i32 {
    let commands: Vec<serde_json::Value> = VERBS
        .iter()
        .map(|(name, summary, effect)| {
            serde_json::json!({
                "name": name,
                "description": summary,
                "x-jbx-effect": effect,
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
        // SAID IN THE DOCUMENT ITSELF, so that `"opencli": "0.1"` above
        // is not read as a promise: the specification is pre-1.0 and
        // several projects publish one under that name. What we can
        // stand behind is the extension, because it is ours.
        "x-jbx-note": "The OpenCLI specification is pre-1.0 and several projects \
                       publish one; this document follows opencli.org and may \
                       change with it. `x-jbx-effect` is ours: it says what a \
                       verb does to the world, which no CLI schema carries, and \
                       it is the part to rely on.",
    });
    outln!("{}", serde_json::to_string_pretty(&document).unwrap_or_default());
    0
}
