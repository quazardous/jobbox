//! THE CONFIGURATION, AND WHAT WINS OVER WHAT.
//!
//! Two YAML files, read once: a project's `.jbx.yaml` and a global one.
//! Everything in them can also be said with an environment variable, and
//! **the variable wins** — a variable is what somebody typed for this
//! one run, a file is what they decided once. Overriding the deliberate
//! with the durable would be backwards.
//!
//! The project file wins over the global one, KEY BY KEY. A project that
//! named one setting must not silence every other setting the global
//! file made — that is an afternoon lost to a file that looks right.
//!
//! ────────────────────────────────────────────────────────────────────
//! WHY A REAL YAML PARSER FOR SIX KEYS
//! ────────────────────────────────────────────────────────────────────
//!
//! Because a hand-rolled reader for "the YAML we support" is a format
//! that LOOKS like YAML and is not. The first quoted string, the first
//! `# comment` after a value, the first `yes` meant as a boolean, and it
//! diverges silently from what the person who wrote the file expected.
//! One maintained dependency costs less than that class of bug.
//!
//! ────────────────────────────────────────────────────────────────────
//! IT NEVER REFUSES TO START
//! ────────────────────────────────────────────────────────────────────
//!
//! A broken file is reported once, on stderr, and then ignored. This is
//! read by a hook that runs before every command on the machine: a
//! configuration error that stopped the shell would be a typo that
//! stopped the day.

use std::path::PathBuf;
use std::sync::OnceLock;

use yaml_rust2::{Yaml, YamlLoader};

/// Where a value came from — the whole point of `jbx config`.
#[derive(Clone, Copy, PartialEq)]
pub enum Source {
    Environment,
    Project,
    Global,
    Default,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Environment => "environment",
            Source::Project => "this project",
            Source::Global => "global config",
            Source::Default => "default",
        }
    }
}

/// WHAT TO DO ABOUT rtk.
///
/// `Auto` calls it when it is reachable and shrugs when it is not.
/// `Always` says so out loud when it is missing, which is what somebody
/// who relies on its savings wants to hear. `Never` leaves it alone
/// entirely — and `init` then leaves its hook alone too, because
/// unregistering a tool we have also decided not to call would remove it
/// from the machine altogether.
#[derive(Clone, Copy, PartialEq)]
pub enum Compose {
    Auto,
    Always,
    Never,
}

impl Compose {
    fn parse(text: &str) -> Option<Compose> {
        match text.trim() {
            "auto" => Some(Compose::Auto),
            "always" | "true" => Some(Compose::Always),
            "never" | "false" => Some(Compose::Never),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Compose::Auto => "auto",
            Compose::Always => "always",
            Compose::Never => "never",
        }
    }
}

/// The file everything is read from.
///
/// `JBX_CONFIG` first, because a test — and a person trying something —
/// needs to point this somewhere that is not their real one. Otherwise
/// the platform's usual place for configuration, which is not where the
/// logs go: one is edited by hand and kept, the other is written by the
/// machine and swept.
pub fn path() -> PathBuf {
    if let Some(named) = std::env::var_os("JBX_CONFIG") {
        return PathBuf::from(named);
    }
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.unwrap_or_else(|| PathBuf::from("."))
        .join("jobbox")
        .join("config.yaml")
}

/// THE PROJECT'S OWN FILE, at the root of the repository it lives in.
///
/// jbx WORKS EVERYWHERE BY DEFAULT — that is the whole design, since a
/// list of projects worth wrapping would be the prediction this tool
/// refuses. This file is how one project says otherwise: a different
/// threshold, or `enabled: false` and jbx stays out of the way there.
///
/// The REPOSITORY ROOT, not the working directory: a setting that
/// changed when you cd into a subdirectory would be a setting nobody
/// could rely on.
pub fn project_path() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let root = project_root();
    let mut at: &std::path::Path = &cwd;
    loop {
        let candidate = at.join(".jbx.yaml");
        if candidate.exists() {
            return Some(candidate);
        }
        if at == root {
            return None; // the project ends here, and it has no file
        }
        at = at.parent()?;
    }
}

/// WHERE A PROJECT BEGINS: `.claude` first, `.git` second.
///
/// A project is not always a repository — a directory Claude Code has
/// been told about is a project whether or not anybody ran `git init`,
/// and looking only for `.git` would put every such directory in the
/// same nameless heap. `.claude` is the marker that says "this is a
/// place somebody works", so it is the one asked first.
///
/// The nearest ancestor holding either wins, which makes a subdirectory
/// of a project part of that project rather than a project of its own.
pub fn project_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut at: &std::path::Path = &cwd;
    loop {
        if at.join(".claude").exists() || at.join(".git").exists() {
            return at.to_path_buf();
        }
        match at.parent() {
            Some(up) => at = up,
            // NOTHING FOUND: the working directory is the project. Better
            // a narrow answer than lumping everything under the root.
            None => return cwd,
        }
    }
}

fn read_yaml(path: &std::path::Path) -> Yaml {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Yaml::BadValue;
    };
    match YamlLoader::load_from_str(&text) {
        Ok(docs) => docs.into_iter().next().unwrap_or(Yaml::BadValue),
        Err(e) => {
            // SAID ONCE, THEN IGNORED. Naming the file matters more than
            // naming the line: whoever sees this is looking at a shell
            // that still works and needs to know where to go.
            eprintln!("jbx: {} is not valid YAML ({e}) — ignoring it", path.display());
            Yaml::BadValue
        }
    }
}

fn global() -> &'static Yaml {
    static LOADED: OnceLock<Yaml> = OnceLock::new();
    LOADED.get_or_init(|| read_yaml(&path()))
}

fn project() -> &'static Yaml {
    static LOADED: OnceLock<Yaml> = OnceLock::new();
    LOADED.get_or_init(|| match project_path() {
        Some(p) => read_yaml(&p),
        None => Yaml::BadValue,
    })
}

fn walk(doc: &'static Yaml, dotted: &str) -> &'static Yaml {
    let mut node = doc;
    for key in dotted.split('.') {
        node = &node[key];
    }
    node
}

/// Walk a dotted path — `integration.rtk.compose` — through the project
/// file first, then the global one, and say which answered.
///
/// KEY BY KEY, NOT FILE BY FILE. A project file that named one setting
/// would otherwise silence every other setting the global file made, and
/// somebody would spend an afternoon on it.
fn at(dotted: &str) -> (&'static Yaml, Source) {
    let local = walk(project(), dotted);
    if !matches!(local, Yaml::BadValue) {
        return (local, Source::Project);
    }
    (walk(global(), dotted), Source::Global)
}

/// A number, from the environment, then the file, then the default.
fn number(var: &str, key: &str, fallback: f64) -> (f64, Source) {
    if let Some(text) = std::env::var(var).ok().filter(|v| !v.is_empty()) {
        if let Ok(value) = text.parse() {
            return (value, Source::Environment);
        }
    }
    let (node, source) = at(key);
    match node {
        Yaml::Integer(n) => (*n as f64, source),
        Yaml::Real(text) => match text.parse() {
            Ok(value) => (value, source),
            Err(_) => (fallback, Source::Default),
        },
        _ => (fallback, Source::Default),
    }
}

fn text_of(node: &Yaml) -> Option<&str> {
    match node {
        Yaml::String(s) => Some(s.as_str()),
        Yaml::Boolean(true) => Some("true"),
        Yaml::Boolean(false) => Some("false"),
        _ => None,
    }
}

/// Seconds a line may hold the caller before it is detached.
pub fn after() -> (f64, Source) {
    number("JBX_AFTER", "after", 30.0)
}

/// Seconds of silence before a running job is called mute.
pub fn mute_after() -> (f64, Source) {
    number("JBX_MUTE_AFTER", "mute_after", 600.0)
}

/// How many QUEUED jobs may run at once; `None` means no cap.
pub fn slots(default: usize) -> (Option<usize>, Source) {
    let read = |text: &str| -> Option<Option<usize>> {
        match text.trim() {
            "none" | "0" => Some(None),
            other => other.parse::<usize>().ok().map(Some),
        }
    };
    if let Some(text) = std::env::var("JBX_SLOTS").ok().filter(|v| !v.is_empty()) {
        if let Some(value) = read(&text) {
            return (value, Source::Environment);
        }
    }
    let (node, source) = at("slots");
    match node {
        Yaml::Integer(n) if *n <= 0 => (None, source),
        Yaml::Integer(n) => (Some(*n as usize), source),
        node => match text_of(node).and_then(read) {
            Some(value) => (value, source),
            None => (Some(default), Source::Default),
        },
    }
}

/// Where logs and records live.
pub fn dir(default: PathBuf) -> (PathBuf, Source) {
    if let Some(named) = std::env::var_os("JBX_DIR") {
        return (PathBuf::from(named), Source::Environment);
    }
    let (node, source) = at("dir");
    match text_of(node) {
        Some(text) if !text.is_empty() => (PathBuf::from(expand(text)), source),
        _ => (default, Source::Default),
    }
}

/// A LEADING `~` IS THE ONE THING PEOPLE WRITE IN A CONFIGURATION FILE
/// AND NO PROGRAM EXPANDS. A shell does it before the program ever sees
/// the word, so a path typed into YAML arrives literally — and a
/// directory called `~` appears in the working directory instead.
fn expand(text: &str) -> String {
    let Some(rest) = text.strip_prefix('~') else { return text.to_string() };
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    match home {
        Some(home) => format!("{}{}", home.to_string_lossy(), rest),
        None => text.to_string(),
    }
}

/// What to do about rtk.
pub fn compose() -> (Compose, Source) {
    if let Some(text) = std::env::var("JBX_RTK").ok().filter(|v| !v.is_empty()) {
        if let Some(value) = Compose::parse(&text) {
            return (value, Source::Environment);
        }
    }
    let (node, source) = at("integration.rtk.compose");
    match text_of(node).and_then(Compose::parse) {
        Some(value) => (value, source),
        None => (Compose::Auto, Source::Default),
    }
}

/// The shell that runs a wrapped line, when the guess is wrong.
///
/// Named rather than derived, because whose shell a line was written for
/// is not something a program can read off the line. `cmd` on Windows
/// means the native one; anything else is used with `-c`.
pub fn shell() -> Option<String> {
    if let Ok(named) = std::env::var("JBX_SHELL") {
        if !named.is_empty() {
            return Some(named);
        }
    }
    text_of(at("shell").0).map(str::to_string).filter(|s| !s.is_empty())
}

/// WHETHER jbx DOES ANYTHING HERE AT ALL.
///
/// True everywhere, and that is deliberate: deciding in advance which
/// projects deserve wrapping is the same guess this tool exists to
/// avoid. `enabled: false` in a project's `.jbx.yaml` is how one place
/// opts out — the hook then says nothing there, and commands run exactly
/// as they would with jbx uninstalled.
pub fn enabled() -> (bool, Source) {
    if let Some(text) = std::env::var("JBX_ENABLED").ok().filter(|v| !v.is_empty()) {
        return (!matches!(text.trim(), "0" | "false" | "no"), Source::Environment);
    }
    let (node, source) = at("enabled");
    match node {
        Yaml::Boolean(value) => (*value, source),
        node => match text_of(node) {
            Some("false") => (false, source),
            Some("true") => (true, source),
            _ => (true, Source::Default),
        },
    }
}

/// WHETHER rtk ANSWERS — asked by running it, not by looking for a file.
///
/// What matters is whether it responds, not whether something with that
/// name sits on the PATH. A stale symlink and a broken build both pass
/// the second test and fail the first.
pub fn rtk_answers() -> bool {
    std::process::Command::new("rtk")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The file `jbx init` leaves at a project's root.
///
/// EVERY LINE COMMENTED BUT ONE. Writing a file that changes nothing is
/// the point: the settings become discoverable by reading rather than by
/// asking, and the one uncommented key is the one worth a decision.
///
/// It says what was FOUND, not what is assumed. A file claiming rtk is
/// composed on a machine that has never had rtk is a file that will
/// mislead somebody in six months — so the comment reports the answer
/// `rtk --version` actually gave, on the day this was written.
pub fn project_template(rtk_found: bool) -> String {
    let seen = if rtk_found {
        "# rtk answered when this file was written, so `auto` composes its\n\
         # rewrite: jbx calls it and wraps the result."
    } else {
        "# rtk did not answer when this file was written. `auto` costs\n\
         # nothing here — it simply finds nothing to compose. Set `always`\n\
         # if this project expects rtk and you want to hear about it when\n\
         # it is missing."
    };
    format!(
        "# JobBox, for this project. jbx works everywhere by default, so\n\
         # everything below is commented: this file changes nothing until\n\
         # you uncomment something. `jbx config` says what is in effect.\n\
         #\n\
         # A setting here beats the global one KEY BY KEY, and an\n\
         # environment variable beats both.\n\
         \n\
         # enabled: false       # jbx stays out of the way in this project\n\
         # after: 30            # seconds before a long line detaches itself\n\
         # slots: 4             # how many QUEUED jobs run at once; `none` for no cap\n\
         \n\
         {seen}\n\
         integration:\n\
         \x20 rtk:\n\
         \x20   compose: auto     # auto | always | never\n"
    )
}

/// The file `jbx init` writes when there is none, so that the settings
/// are discoverable by reading rather than by asking.
pub const TEMPLATE: &str = "\
# JobBox — every setting it has, with its default.
# Anything here can also be set by an environment variable, and the
# variable wins: it is what you typed for one run, this is what you
# decided once.

# after: 30            # seconds before a long line detaches itself
# mute_after: 600      # seconds of silence before a running job is called mute
# slots: 4             # how many QUEUED jobs run at once; `none` for no cap
# dir: ~/.cache/jbx    # where logs and records live

integration:
  rtk:
    # auto   — compose rtk's rewrite when rtk is reachable (the default)
    # always — the same, and say so loudly when it is missing
    # never  — leave rtk alone entirely; `jbx init` then leaves its
    #          own hook registered, because unregistering a tool we
    #          have also decided not to call would remove it outright
    compose: auto
";
