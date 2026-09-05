//! WHAT ACTUALLY TAKES TIME, PER PROJECT.
//!
//! One line is appended per wrapped command, by the supervisor — which
//! is the only party that knows both the true duration and the exit
//! code, for every line, detached or not. The front process cannot: when
//! it lets go, the command has not finished.
//!
//! THE LINE AS TYPED IS NEVER STORED. A command line carries secrets —
//! an inline `TOKEN=… ./deploy` is ordinary — and this table sits in a
//! cache directory for weeks. What a reading needs is the SHAPE anyway:
//! which kinds of command are long, not which exact string was.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::store;

/// Shells, so that `-c` can be read as "the rest is somebody's script".
const SHELLS: [&str; 6] = ["sh", "bash", "zsh", "dash", "ksh", "fish"];

/// A COMMAND'S SHAPE, NOT THE COMMAND.
///
/// Three tokens is the grain that separates what matters — `make test`
/// from `make test-all`, `git status` from `git push` — while `bash -c`
/// collapses everything behind it, which is right: what follows is
/// somebody else's script and its shape says nothing about ours.
///
/// Assignments are DROPPED rather than truncated: `FOO=bar` is where a
/// secret lives, and a truncated secret is still a leaked prefix.
///
/// These are the same rules the Python side already uses for its own
/// table. Two tools measuring the same machine with different grains
/// would produce two answers to one question.
pub fn fingerprint(command: &str) -> String {
    let command = strip_leading_cd(command.trim());
    // A LEADING `rtk` IS TRANSPORT, NOT IDENTITY. The hook adds it, a
    // hand-typed line does not, and keeping it would file one command
    // under two shapes depending on which door it came through — which
    // is the one thing a grouping must not do.
    let command = command.strip_prefix("rtk ").unwrap_or(command).trim_start();
    let tokens: Vec<&str> = command
        .split_whitespace()
        .filter(|t| !t.split('/').next().unwrap_or(t).contains('='))
        .collect();
    if tokens.len() > 1 && tokens[1] == "-c" {
        if let Some(name) = base(tokens[0]) {
            if SHELLS.contains(&name.as_str()) {
                return format!("{name} -c");
            }
        }
    }
    let shape = tokens
        .iter()
        .take(3)
        .map(|t| t.chars().take(24).collect::<String>())
        .collect::<Vec<_>>()
        .join(" ");
    if shape.is_empty() { "?".into() } else { shape }
}

/// A HARNESS PREFIXES COMMANDS WITH A `cd` INTO THE WORKING DIRECTORY,
/// and grouping on that would group everything under one shape. What is
/// asked is which COMMAND is long; the directory is not part of it.
fn strip_leading_cd(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("cd ") else { return text };
    let rest = rest.trim_start();
    let mut cut = rest.len();
    for sep in ["&&", ";", "\n"] {
        if let Some(found) = rest.find(sep) {
            cut = cut.min(found + sep.len());
        }
    }
    if cut < rest.len() { rest[cut..].trim_start() } else { text }
}

fn base(path: &str) -> Option<String> {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned())
}

/// THE PROJECT A COMMAND BELONGS TO — see `config::project_root` for
/// where one begins, and why `.claude` is asked before `.git`.
pub fn project() -> (String, String) {
    // ONE DEFINITION OF "PROJECT", in `config`, because the name a
    // reading is filed under and the directory a `.jbx.yaml` is looked
    // for in have to be the same place. Two walks that disagreed would
    // put a project's settings on one row and its readings on another.
    let at = crate::config::project_root();
    let name = at
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "?".into());
    (name, at.display().to_string())
}

fn table_path() -> PathBuf {
    store::dir().join("stats.jsonl")
}

/// Append one reading of a line that ran. Called by the supervisor,
/// once, at the very end — it is the only party that knows both the true
/// duration and the exit code.
pub fn record(shape: &str, secs: f64, after: f64, fg: bool, code: i32) {
    let (name, path) = project();
    append(serde_json::json!({
        "at": store::now(),
        "kind": "run",
        // A DELIBERATE FOREGROUND IS NOT A FAILURE TO DETACH, and the
        // table must be able to tell them apart. One is somebody saying
        // they need the answer; the other is a line that simply was not
        // long enough to bother.
        "fg": fg,
        "project": name,
        "path": path,
        "shape": shape,
        "secs": (secs * 100.0).round() / 100.0,
        "after": after,
        "code": code,
    }));
}

/// TIME SPENT WAITING FOR A LINE THAT HAD ALREADY BEEN LET GO OF.
///
/// THIS IS WHAT KEEPS THE HEADLINE HONEST. Detaching a line makes its
/// remaining time AVAILABLE; it does not make it saved. Whoever answers
/// the detachment message with `jbx wait` spends that time anyway, and a
/// tool that counted it as a gain would be reporting its own good
/// intentions.
///
/// So every block is written down and subtracted. What is left is time
/// the caller genuinely did something else with — or at least, time this
/// tool can prove it did not spend here.
pub fn record_wait(secs: f64) {
    let (name, path) = project();
    append(serde_json::json!({
        "at": store::now(),
        "kind": "wait",
        "project": name,
        "path": path,
        "secs": (secs * 100.0).round() / 100.0,
    }));
}

/// IT NEVER FAILS OUT LOUD. This runs after every command on the
/// machine; a measurement that can break a command is not worth taking.
/// One `write` of a short line to a file opened for append is atomic on
/// both platforms, which is what lets a hundred of these run at once
/// without a lock.
fn append(line: Value) {
    if let Ok(mut file) = OpenOptions::new().append(true).create(true).open(table_path()) {
        let _ = writeln!(file, "{line}");
    }
}

struct Reading {
    project: String,
    path: String,
    shape: String,
    secs: f64,
    after: f64,
    /// `true` when the caller asked for the foreground on purpose.
    fg: bool,
    /// `true` for a line that ran, `false` for time spent blocked on one
    /// that had already been detached. Readings written before this
    /// existed have no `kind` and are runs, which is what they were.
    ran: bool,
}

fn read_all() -> Vec<Reading> {
    let Ok(text) = std::fs::read_to_string(table_path()) else { return Vec::new() };
    text.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .map(|v| Reading {
            project: v["project"].as_str().unwrap_or("?").into(),
            path: v["path"].as_str().unwrap_or("").into(),
            shape: v["shape"].as_str().unwrap_or("?").into(),
            secs: v["secs"].as_f64().unwrap_or(0.0),
            after: v["after"].as_f64().unwrap_or(f64::MAX),
            ran: v["kind"].as_str().unwrap_or("run") == "run",
            fg: v["fg"].as_bool().unwrap_or(false),
        })
        .collect()
}

#[derive(Default)]
struct Tally {
    calls: usize,
    detached: usize,
    /// Calls that chose to stand still, and what that came to.
    chosen: usize,
    chosen_secs: f64,
    /// Everything the lines really took, end to end.
    elapsed: f64,
    /// What the caller actually stood still for.
    blocked: f64,
    worst: f64,
    paths: std::collections::BTreeSet<String>,
}

impl Tally {
    fn add(&mut self, r: &Reading) {
        if !r.ran {
            // A BLOCK IS NOT A CALL. It adds to what was stood through
            // and to nothing else — counting it as a call would inflate
            // the denominator with time that was already counted once.
            self.blocked += r.secs;
            return;
        }
        self.calls += 1;
        self.elapsed += r.secs;
        if r.fg {
            self.chosen += 1;
            self.chosen_secs += r.secs;
        }
        // WHAT WAS STOOD THROUGH IS NOT WHAT IT TOOK. A detached line
        // costs the caller the threshold and not a second more; the rest
        // of its duration happened while they were free.
        self.blocked += r.secs.min(r.after);
        if r.secs > r.after {
            self.detached += 1;
        }
        self.worst = self.worst.max(r.secs);
        if !r.path.is_empty() {
            self.paths.insert(r.path.clone());
        }
    }

    /// THE HEADLINE: how much of the elapsed time the caller did not
    /// stand through — `saved`, and the footer says in what sense.
    /// Never negative — waiting twice on one job can put
    /// more on the clock than the job ever took, and a negative
    /// compression would be arithmetic pretending to be a finding.
    fn saved(&self) -> f64 {
        (self.elapsed - self.blocked).max(0.0)
    }

    fn ratio(&self) -> f64 {
        if self.elapsed <= 0.0 {
            0.0
        } else {
            self.saved() / self.elapsed
        }
    }
}

/// LAY THE PROJECTS OUT AS THEY SIT ON DISK.
///
/// Projects nest — a repository inside a repository, a sub-module, a tool
/// living in the tree of the thing it serves. A flat list hides that, and
/// hides it exactly where it matters: three of the four rows on this
/// machine live inside the fourth.
///
/// So a project whose path is under another project's path is shown as
/// its child, by the part of the path that differs. A name that would
/// still be ambiguous — two `api` under unrelated parents — gets four
/// characters of the path's hash, which is what the Python this replaces
/// did, and it is only spent where it is needed.
fn arrange(
    by: &BTreeMap<String, Tally>,
    names: &BTreeMap<String, String>,
    full_path: bool,
) -> Vec<Vec<String>> {
    let paths: Vec<&String> = by.keys().collect();
    // The nearest other row that contains this one, if any.
    let parent = |p: &str| -> Option<String> {
        paths
            .iter()
            .filter(|other| p.starts_with(&format!("{other}/")))
            .max_by_key(|other| other.len())
            .map(|s| s.to_string())
    };

    let mut label: BTreeMap<&String, String> = BTreeMap::new();
    for p in &paths {
        let shown = match parent(p) {
            // The part below the parent, so a nested tool reads as what
            // it is called and not as the whole road to it.
            Some(up) => p[up.len() + 1..].to_string(),
            None => names.get(*p).cloned().unwrap_or_else(|| (*p).clone()),
        };
        label.insert(p, shown);
    }
    // FOUR HEX CHARACTERS, AND ONLY WHERE THEY EARN THEIR PLACE. A column
    // of hashes nobody needs is a column nobody reads.
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for p in &paths {
        *seen.entry(label[*p].clone()).or_default() += 1;
    }
    for p in &paths {
        if seen[&label[*p]] > 1 {
            let short = fingerprint_path(p);
            label.insert(p, format!("{}-{short}", label[*p]));
        }
    }

    // Depth first, and heaviest first among siblings.
    let mut order: Vec<&String> = paths.clone();
    order.sort_by(|a, b| by[*b].saved().total_cmp(&by[*a].saved()));
    let layout = Layout { order: &order, parent: &parent, by, label: &label, full_path };
    let mut rows = Vec::new();
    layout.walk(None, 0, &mut rows);
    rows
}

/// Everything the walk needs, carried once instead of passed eight times.
struct Layout<'a> {
    order: &'a [&'a String],
    parent: &'a dyn Fn(&str) -> Option<String>,
    by: &'a BTreeMap<String, Tally>,
    label: &'a BTreeMap<&'a String, String>,
    full_path: bool,
}

impl Layout<'_> {
    fn walk(&self, here: Option<&String>, depth: usize, rows: &mut Vec<Vec<String>>) {
        for p in self.order {
            if (self.parent)(p).as_ref() != here {
                continue;
            }
            let t = &self.by[*p];
            let shown = if self.full_path {
                (*p).clone()
            } else {
                format!("{}{}", "  ".repeat(depth), self.label[*p])
            };
            rows.push(vec![
                shown,
                t.calls.to_string(),
                t.detached.to_string(),
                human(t.elapsed),
                human(t.blocked),
                format!("{} ({:.0}%)", human(t.saved()), t.ratio() * 100.0),
            ]);
            self.walk(Some(p), depth + 1, rows);
        }
    }
}

/// Four hex characters of a path, to tell two same-named projects apart.
///
/// FNV-1a, written out rather than pulled in: a hash used only to make a
/// label unique needs no more than that, and a dependency for four
/// characters is a dependency to keep for ever.
fn fingerprint_path(path: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{:04x}", (hash & 0xffff) as u16)
}

/// A duration a person can read at a glance.
fn human(secs: f64) -> String {
    let s = secs.round() as i64;
    if s >= 3600 {
        format!("{}h{:02}m", s / 3600, (s % 3600) / 60)
    } else if s >= 60 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else if secs >= 10.0 {
        format!("{s}s")
    } else {
        format!("{secs:.1}s")
    }
}

fn row(cells: &[String], widths: &[usize]) -> String {
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        if i + 1 == cells.len() {
            let _ = write!(out, "{cell}");
        } else {
            let _ = write!(out, "{:<width$}  ", cell, width = widths[i]);
        }
    }
    out
}

fn print_table(headings: &[&str], rows: Vec<Vec<String>>) {
    let mut widths: Vec<usize> = headings.iter().map(|h| h.chars().count()).collect();
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    outln!("{}", row(&headings.iter().map(|h| h.to_string()).collect::<Vec<_>>(), &widths));
    for r in rows {
        outln!("{}", row(&r, &widths));
    }
}

/// `jbx stats` — HOW MUCH TIME WAS COMPRESSED, per project.
///
/// The one number this tool is for. `elapsed` is what the lines really
/// took; `blocked` is what the caller stood through. The difference is
/// time that happened while they were free — and it is the difference,
/// not the elapsed total, that says whether wrapping everything was
/// worth it.
///
/// IT IS DELIBERATELY NOT CALLED "SAVED". Time made available is only
/// saved if something else was done with it. What is subtracted here is
/// every block this tool can SEE — the `jbx wait` calls — and it cannot
/// see somebody staring at the screen instead. So the number is an upper
/// bound that has been made as tight as the evidence allows, and the
/// footer says so rather than letting the column imply otherwise.
pub fn stats(only: Option<&str>, full_path: bool) -> i32 {
    let readings = read_all();
    if readings.is_empty() {
        outln!("nothing measured yet — the table fills as commands run through `jbx run`.");
        return 0;
    }

    match only {
        None => {
            // GROUPED BY PATH, NOT BY NAME. Two directories called `api`
            // are two projects, and summing them made one row whose every
            // number was the sum of two unrelated things.
            let mut by: BTreeMap<String, Tally> = BTreeMap::new();
            let mut names: BTreeMap<String, String> = BTreeMap::new();
            let mut all = Tally::default();
            for r in &readings {
                let key = if r.path.is_empty() { r.project.clone() } else { r.path.clone() };
                by.entry(key.clone()).or_default().add(r);
                names.entry(key).or_insert_with(|| r.project.clone());
                all.add(r);
            }
            print_table(
                &["project", "calls", "detached", "elapsed", "blocked", "saved"],
                arrange(&by, &names, full_path),
            );
            outln!();
            outln!(
                "{} saved — command time that ran while the caller was free, {:.0}% of {}.",
                human(all.saved()),
                all.ratio() * 100.0,
                human(all.elapsed)
            );
            if all.chosen > 0 {
                // THE DELIBERATE FOREGROUND, COUNTED. Choosing it is
                // legitimate and sometimes right; a HABIT of choosing it
                // is what is worth seeing, and it is invisible unless
                // somebody adds it up.
                outln!(
                    "{} of {} calls asked for the foreground on purpose, costing {}.",
                    all.chosen,
                    all.calls,
                    human(all.chosen_secs)
                );
            }
            outln!("`saved` already subtracts the time you gave back to `jbx wait`. It cannot");
            outln!("see you waiting some other way, so read it as a ceiling, not a receipt.");
            outln!("Name a project to see its shapes; `--project-path` for full paths.");
        }
        Some(want) => {
            let mut by: BTreeMap<String, Tally> = BTreeMap::new();
            let mut seen = false;
            // A NAME, A PATH, OR THE TAIL OF ONE. Whatever the table
            // showed you is what you should be able to type back.
            let matches = |r: &Reading| {
                r.ran
                    && (r.project == want
                        || r.path == want
                        || r.path.ends_with(&format!("/{want}")))
            };
            for r in readings.iter().filter(|r| matches(r)) {
                seen = true;
                by.entry(r.shape.clone()).or_default().add(r);
            }
            if !seen {
                eprintln!("jbx: nothing measured for {want:?}");
                return 1;
            }
            let mut rows: Vec<(f64, Vec<String>)> = by
                .into_iter()
                .map(|(shape, t)| {
                    (t.saved(), vec![
                        shape,
                        t.calls.to_string(),
                        t.detached.to_string(),
                        human(t.worst),
                        human(t.saved()),
                    ])
                })
                .collect();
            rows.sort_by(|a, b| b.0.total_cmp(&a.0));
            print_table(
                &["shape", "calls", "detached", "worst", "saved"],
                rows.into_iter().map(|(_, r)| r).collect(),
            );
            outln!();
            // PER SHAPE, THE BLOCKS CANNOT BE ATTRIBUTED: a `jbx wait`
            // names a job, not the shape it came from. Saying so beats
            // quietly spreading them over the rows.
            outln!("Per shape, `saved` does not subtract time given back to `jbx wait` —");
            outln!("a block names a job, not a shape. The project total above does subtract it.");
        }
    }
    0
}

/// Drop readings older than `days`, but only once the table is big
/// enough for it to matter.
///
/// REWRITING A FILE THAT EVERY COMMAND APPENDS TO IS THE ONE RACE HERE,
/// so it is done rarely and never on a small table: below a megabyte
/// there is nothing to gain and a concurrent append to lose.
pub fn forget_older_than(days: f64) {
    let path = table_path();
    let Ok(meta) = std::fs::metadata(&path) else { return };
    if meta.len() < 1_048_576 {
        return;
    }
    let cut = store::now() - days * 86400.0;
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| {
            serde_json::from_str::<Value>(l)
                .ok()
                .and_then(|v| v["at"].as_f64())
                .map(|at| at >= cut)
                .unwrap_or(false)
        })
        .collect();
    if kept.len() == text.lines().count() {
        return;
    }
    let tmp = path.with_extension("jsonl.part");
    if std::fs::write(&tmp, kept.join("\n") + "\n").is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}
