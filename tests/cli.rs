//! THE TESTS GO THROUGH THE COMMAND, NEVER THROUGH THE FUNCTION.
//!
//! This tool's whole value is what happens between processes: a child
//! that outlives its parent, a code written by one and read by another,
//! a hook answering a harness on standard output. A test calling the
//! functions directly would prove the pieces and miss every wire — and
//! wires are all there is here.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// The binary, by the path cargo built it at. Naming it plainly would
/// test whichever copy the PATH happens to hold, which on this machine
/// is a different version of the same tool.
const JBX: &str = env!("CARGO_BIN_EXE_jbx");

/// The binary's path AS IT MUST APPEAR INSIDE A LINE WE HAND TO A
/// SHELL.
///
/// `CARGO_BIN_EXE_jbx` is a native path, so on Windows it arrives with
/// backslashes — and bash eats those as escapes before the line ever
/// runs. MEASURED: `bash -c "echo D:\a\jobbox\target\release\jbx.exe"`
/// prints `D:ajobboxtargetreleasejbx.exe`, so the line meant to start
/// jbx started nothing at all — and said so to no one. Three tests
/// never noticed, because what they assert is that nothing CRASHED; the
/// fourth wanted the job to still be running, and was the only one that
/// ever failed. Windows accepts a forward slash everywhere it accepts a
/// backslash, so this is the one spelling both shells read alike.
fn jbx_in_line() -> String {
    JBX.replace('\\', "/")
}

/// ASK, DO NOT SLEEP.
///
/// A fixed pause encodes a guess about how fast the machine is, and the
/// Windows runner is slower than every guess this suite made: eleven
/// tests failed on that alone. This polls for the thing it is waiting
/// for and gives up loudly, so a real hang still FAILS rather than
/// hanging — the deadline is a backstop, not a timing assumption.
fn until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        if ready() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("waited 20s and {what} never happened");
}

/// THE ID OUT OF AN ANNOUNCEMENT, WITHOUT KNOWING ITS WORDING.
///
/// Ten tests used to split on the literal `"detached as "`, so shortening
/// that message — thirteen lines of argument down to three — broke them
/// all at once, on an `unwrap` of `None` that named a line number and
/// nothing else. An id has a shape and the sentence around it does not:
/// `j` and seven hex digits, which no English word is.
fn announced(said: &str) -> String {
    said.split(|c: char| !c.is_ascii_alphanumeric())
        .find(|w| {
            w.len() == 8 && w.starts_with('j') && w[1..].chars().all(|c| c.is_ascii_hexdigit())
        })
        .unwrap_or_else(|| panic!("no job id was announced in:\n{said}"))
        .to_string()
}

/// A scratch home for one test. Each has its own, because the tests run
/// at the same time and a shared store would let one test's job appear
/// in another's `list`.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("jbx-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(JBX)
        .env_remove("JBX_WRAPPED")
            .args(args)
            .env("JBX_DIR", &self.0)
            .env("JBX_CONFIG", self.0.join("global.yaml"))
            .stdin(Stdio::null())
            .output()
            .expect("the binary runs")
    }
    /// The same, under a named client — a mailbox address, so two of
    /// these are two sessions as far as the endings are concerned.
    fn run_as(&self, who: &str, args: &[&str]) -> Output {
        Command::new(JBX)
        .env_remove("JBX_WRAPPED")
            .args(args)
            .env("JBX_DIR", &self.0)
            .env("JBX_CONFIG", self.0.join("global.yaml"))
            .env("JBX_CLIENT", who)
            .stdin(Stdio::null())
            .output()
            .expect("the binary runs")
    }

    /// STOP WHAT THIS TEST STARTED.
    ///
    /// The long-lived jobs exist so a listing can be read while they are
    /// still running; they have no reason to outlive the reading. On a
    /// runner that costs nothing, and on somebody's machine it is a
    /// handful of `sleep 30` left behind by `cargo test`.
    fn stop_everything(&self) {
        // `--all`, BECAUSE A TEST CAN RUN A JOB FROM ANOTHER PROJECT: the
        // width test does, from a long-named directory, and a scoped list
        // never showed that job — so it was never stopped, and outlived the
        // suite. The store is this test's own, so `--all` reaches nothing else.
        let listed = text(&self.run(&["list", "--all", "--json"]));
        let Ok(rows) = serde_json::from_str::<serde_json::Value>(listed.trim()) else { return };
        for row in rows.as_array().map(Vec::as_slice).unwrap_or_default() {
            if let Some(id) = row["id"].as_str() {
                let _ = self.run(&["kill", id]);
            }
        }
    }

    /// Feed one harness event to the hook and take what it answers.
    fn event(&self, who: &str, json: &str) -> String {
        use std::io::Write;
        let mut child = Command::new(JBX)
        .env_remove("JBX_WRAPPED")
            .arg("hook")
            .env("JBX_DIR", &self.0)
            .env("JBX_CONFIG", self.0.join("global.yaml"))
            .env("JBX_CLIENT", who)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(json.as_bytes()).unwrap();
        String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).into_owned()
    }

    /// The same, but naming the dialect and keeping the whole result —
    /// a refusal is an exit code and a line on stderr, neither of which
    /// `event` can show.
    fn event_as(&self, dialect: &str, json: &str) -> Output {
        use std::io::Write;
        let mut child = Command::new(JBX)
            .env_remove("JBX_WRAPPED")
            .args(["hook", dialect])
            .env("JBX_DIR", &self.0)
            .env("JBX_CONFIG", self.0.join("global.yaml"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(json.as_bytes()).unwrap();
        child.wait_with_output().unwrap()
    }

    /// The same, with one extra variable set — a threshold, a cap.
    fn run_with(&self, env: &[(&str, &str)], args: &[&str]) -> Output {
        let mut cmd = Command::new(JBX);
        cmd.env_remove("JBX_WRAPPED")
            .args(args)
            .env("JBX_DIR", &self.0)
            .env("JBX_CONFIG", self.0.join("global.yaml"))
            .env("JBX_CLIENT", "me");
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::null()).output().expect("the binary runs")
    }

    /// A directory that reads as a project — `.claude` is the marker,
    /// because a project is not always a repository — with an optional
    /// `.jbx.yaml` in it. Commands run from `where`, which may be a
    /// subdirectory, so the walk upwards is exercised too.
    fn project(&self, jbx_yaml: Option<&str>, deeper: &str) -> PathBuf {
        let root = self.0.join("project");
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        if let Some(text) = jbx_yaml {
            std::fs::write(root.join(".jbx.yaml"), text).unwrap();
        }
        let here = if deeper.is_empty() { root.clone() } else { root.join(deeper) };
        std::fs::create_dir_all(&here).unwrap();
        here
    }

    /// Run from a directory, with a global config file of our own.
    fn run_in(&self, here: &std::path::Path, global: &str, args: &[&str]) -> Output {
        let config = self.0.join("global.yaml");
        std::fs::write(&config, global).unwrap();
        Command::new(JBX)
        .env_remove("JBX_WRAPPED")
            .args(args)
            .current_dir(here)
            .env("JBX_DIR", &self.0)
            .env("JBX_CONFIG", &config)
            .env_remove("JBX_AFTER")
            .stdin(Stdio::null())
            .output()
            .expect("the binary runs")
    }

    /// The throwaway half of the store. Named `cache` since jbx moved
    /// house: the readings and the record of displaced hooks now sit
    /// beside it rather than inside it, because `~/.cache` may be emptied
    /// at any hour and neither of those can be had again.
    fn jobs(&self) -> PathBuf {
        self.0.join("cache")
    }

    /// The durable half — readings, displaced hooks, settings.
    fn home(&self) -> PathBuf {
        self.0.clone()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_short_line_is_transparent() {
    let s = Scratch::new("short");
    let out = s.run(&["run", "--", "echo straight through"]);
    assert_eq!(text(&out), "straight through\n");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn the_exit_code_of_a_short_line_is_returned_unchanged() {
    let s = Scratch::new("code");
    for code in [1, 2, 42] {
        let out = s.run(&["run", "--", &format!("exit {code}")]);
        assert_eq!(out.status.code(), Some(code), "exit {code} came back wrong");
    }
}

#[test]
fn a_line_that_finishes_in_time_leaves_no_job_behind() {
    let s = Scratch::new("clean");
    s.run(&["run", "--", "echo x"]);
    // THE MEASUREMENT STAYS, THE JOB DOES NOT. `stats.jsonl` is the
    // point of the exercise and outlives every command; a log, a record
    // and an exit code belong to a job, and a line that finished in time
    // never became one.
    let left: Vec<String> = std::fs::read_dir(s.jobs())
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n != "stats.jsonl")
                .collect()
        })
        .unwrap_or_default();
    assert!(left.is_empty(), "job traces left behind: {left:?}");
}

#[test]
fn a_long_line_is_detached_and_named() {
    let s = Scratch::new("detach");
    let out = s.run(&["run", "--after", "1", "--", "sleep 4; exit 3"]);
    let said = text(&out);
    assert!(said.contains("BACKGROUND as j"), "not announced: {said}");
    assert_eq!(out.status.code(), Some(0), "detaching is not a failure");

    let id = announced(&said);

    // THE CODE IS DEFERRED, NOT LOST — and `wait` is what turns the
    // deferral back into a number.
    let waited = s.run(&["wait", &id]);
    assert_eq!(waited.status.code(), Some(3), "the real code did not come back");
    assert!(text(&s.run(&["status", &id])).contains("exit 3"));
}

#[test]
fn output_arrives_before_the_line_ends() {
    let s = Scratch::new("live");
    // It prints, then sleeps past the threshold. If output were replayed
    // at the end rather than poured through, the first line could not be
    // in what we are handed at detachment time.
    let out = s.run(&["run", "--after", "1", "--", "echo early; sleep 4"]);
    let said = text(&out);
    assert!(said.starts_with("early\n"), "output was held back: {said:?}");
    assert!(said.contains("BACKGROUND as j"));
}

#[test]
fn a_detached_line_keeps_its_log() {
    let s = Scratch::new("log");
    let said = text(&s.run(&["run", "--after", "1", "--", "echo one; sleep 2; echo two"]));
    let id = announced(&said);
    s.run(&["wait", &id]);
    let log = text(&s.run(&["tail", &id]));
    assert!(log.contains("one") && log.contains("two"), "log lost half of it: {log:?}");
}

// ── THE HOOK ────────────────────────────────────────────────────────────

fn hook(s: &Scratch, command: &str) -> String {
    hook_as(s, "claude", "PreToolUse", "Bash", command)
}

/// The same, said in another client's dialect.
fn hook_as(s: &Scratch, client: &str, event_name: &str, tool: &str, command: &str) -> String {
    let event = serde_json::json!({
        "hook_event_name": event_name,
        "tool_name": tool,
        "tool_input": {"command": command, "description": "d", "timeout": 120000},
    })
    .to_string();
    let mut child = Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .arg("hook")
        .arg(client)
        .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
        // rtk MUST NOT BE FOUND during the tests: what it rewrites is its
        // business and its version's, and a test that depended on it
        // would fail the day it learns a new command.
        .env("PATH", "/nonexistent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(event.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_hook_wraps_the_whole_line() {
    let s = Scratch::new("hook");
    let answer: serde_json::Value = serde_json::from_str(hook(&s, "make test && ./deploy.sh").trim()).unwrap();
    let rewritten = answer["hookSpecificOutput"]["updatedInput"]["command"].as_str().unwrap();
    // ONE WRAPPER FOR THE WHOLE LINE. Wrapped command by command, `make`
    // would hand `deploy.sh` a detachment code of 0 and send it against
    // a tree that was never built.
    assert_eq!(rewritten.matches("run --").count(), 1, "wrapped in pieces: {rewritten}");
    assert!(rewritten.contains("make test && ./deploy.sh"));
}

#[test]
fn the_hook_echoes_every_field_it_was_given() {
    let s = Scratch::new("fields");
    let answer: serde_json::Value = serde_json::from_str(hook(&s, "ls").trim()).unwrap();
    let updated = &answer["hookSpecificOutput"]["updatedInput"];
    // THE HARNESS REPLACES THE WHOLE OBJECT, so a field left out is a
    // field deleted — `timeout` above all, which is the caller saying
    // how long they were prepared to wait.
    assert_eq!(updated["timeout"], 120000);
    assert_eq!(updated["description"], "d");
    // AND `permissionDecision` STAYS ABSENT: set to "allow" beside an
    // `updatedInput`, the harness drops the rewrite without a word.
    assert!(answer["hookSpecificOutput"].get("permissionDecision").is_none());
}

#[test]
fn the_hook_does_not_wrap_a_wrapped_line() {
    let s = Scratch::new("idem");
    let already = format!("{JBX} run -- 'git status'");
    assert!(hook(&s, &already).trim().is_empty(), "it wrapped its own output");
}

#[test]
fn the_hook_says_nothing_about_other_tools() {
    let s = Scratch::new("other");
    let event = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Read",
        "tool_input": {"file_path": "/etc/hosts"},
    })
    .to_string();
    let mut child = Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .arg("hook")
        .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(event.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.stdout.is_empty(), "it spoke about a tool it does not wrap");
}

#[test]
fn a_quoted_line_still_means_what_it_meant() {
    let s = Scratch::new("quote");
    // AN APOSTROPHE IS THE WHOLE TEST. Quoting a line into a single
    // shell word is where a wrapper silently changes a command, and a
    // French comment or a `don't` is enough to do it.
    let tricky = r#"echo "don't stop, it's summer""#;
    let answer: serde_json::Value = serde_json::from_str(hook(&s, tricky).trim()).unwrap();
    let rewritten = answer["hookSpecificOutput"]["updatedInput"]["command"].as_str().unwrap();
    let through = Command::new("sh")
        .env_remove("JBX_WRAPPED").arg("-c").arg(rewritten).env("JBX_DIR", &s.0).output().unwrap();
    let direct = Command::new("sh")
        .env_remove("JBX_WRAPPED").arg("-c").arg(tricky).output().unwrap();
    assert_eq!(through.stdout, direct.stdout, "quoting changed the command");
}

// ── WIRING IT IN, AND OUT ───────────────────────────────────────────────

#[test]
fn init_leaves_a_commented_project_file_that_changes_nothing() {
    let s = Scratch::new("initproject");
    let here = s.project(None, "");
    let config = s.0.join("claude");
    std::fs::create_dir_all(&config).unwrap();

    let out = Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .arg("init")
        .current_dir(&here)
        .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
        .env("CLAUDE_CONFIG_DIR", &config)
        .output()
        .unwrap();
    assert!(text(&out).contains(".jbx.yaml"), "no project file: {}", text(&out));

    let written = std::fs::read_to_string(here.join(".jbx.yaml")).unwrap();
    // WRITING A FILE THAT CHANGES NOTHING IS THE POINT: the settings
    // become findable by reading rather than by asking, and the one
    // uncommented key is the one worth a decision.
    assert!(written.contains("compose: auto"));
    for opinionated in ["enabled: false", "after: 30", "slots: 4"] {
        let line = written.lines().find(|l| l.contains(opinionated)).unwrap_or("");
        assert!(line.trim_start().starts_with('#'), "{opinionated} was not commented");
    }
    // AND IT MUST PARSE. A template that is not valid YAML would be
    // reported as broken on the first command run in the project.
    let shown = text(&Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .arg("config")
        .current_dir(&here)
        .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
        .output()
        .unwrap());
    assert!(shown.contains("compose"), "the written file did not read back: {shown}");
    assert!(!shown.contains("not valid YAML"));
}

#[test]
fn init_displaces_rtk_and_undo_puts_it_back() {
    let s = Scratch::new("init");
    let config = s.0.join("claude");
    std::fs::create_dir_all(&config).unwrap();
    let settings = config.join("settings.json");
    let before = r#"{"model":"opus","hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"rtk hook claude"}]}]}}"#;
    std::fs::write(&settings, before).unwrap();

    // EVERY PATH THIS TOUCHES IS PINNED INTO THE SCRATCH, and that is
    // not tidiness: without `JBX_CONFIG` this wrote the real
    // `~/.config/jobbox/config.yaml`, and without `current_dir` it
    // dropped a `.jbx.yaml` into the repository being tested. A suite
    // that edits the machine it runs on is a suite nobody can trust.
    let here = s.project(None, "");
    let wired = Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .arg("init")
        .current_dir(&here)
        .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
        .env("CLAUDE_CONFIG_DIR", &config)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&wired.stdout).contains("displaced"));
    let now: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    let entries = now["hooks"]["PreToolUse"][0]["hooks"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "two hooks would race for one field");
    // What is declared is `jbx`, the hot half — never `jbxctl`, which has
    // no `hook` verb and would answer a harness with a usage error on
    // every single command.
    let declared = entries[0]["command"].as_str().unwrap();
    // `jbx hook claude`, OR `jbx.exe hook claude` — the extension is
    // Windows's and not a different binary. The test used to spell the
    // Unix name and called a correct declaration wrong.
    //
    // AND THE DIALECT IS NAMED, which this used to allow to be absent. A
    // bare `jbx hook` answers as Claude, which is right here and silently
    // wrong for every other client — so the name is written even where
    // the default would have been correct.
    assert!(declared.contains("jbx") && declared.ends_with(" hook claude"),
            "declared the wrong binary: {declared}");
    // WHAT WAS NOT OURS IS UNTOUCHED.
    assert_eq!(now["model"], "opus");

    Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .args(["init", "--undo"])
        .current_dir(&here)
        .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
        .env("CLAUDE_CONFIG_DIR", &config)
        .output()
        .unwrap();
    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    // REMOVING THIS TOOL MUST NOT LEAVE A MACHINE WITH NEITHER IT NOR rtk.
    assert_eq!(after, serde_json::from_str::<serde_json::Value>(before).unwrap());
}

// ── WHAT IT MEASURES ────────────────────────────────────────────────────

/// WAIT FOR AN ENDING WITHOUT COLLECTING IT.
///
/// `jbx wait` would do, and used to be what these tests used — but it
/// now clears the ending it delivered, which is the whole point of it.
/// A test that only needs the job to be OVER must not also collect its
/// mail, or it measures the collection rather than the announcement.
fn ended(s: &Scratch, id: &str) {
    until("the ending landed", || {
        signals_of(s).iter().any(|v| v["id"].as_str() == Some(id))
    });
}

/// The agent's unread endings, parsed.
fn signals_of(s: &Scratch) -> Vec<serde_json::Value> {
    let dir = s.jobs().join("signals");
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in entries.flatten().filter(|e| e.path().is_dir()) {
        let raw = std::fs::read_to_string(e.path().join("agent.jsonl")).unwrap_or_default();
        out.extend(raw.lines().filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok()));
    }
    out
}

fn readings(s: &Scratch) -> Vec<serde_json::Value> {
    std::fs::read_to_string(s.home().join("readings.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[test]
fn every_line_leaves_a_reading_even_the_short_ones() {
    let s = Scratch::new("gain");
    s.run(&["run", "--", "echo one"]);
    s.run(&["run", "--", "echo two"]);
    // A SHORT LINE LEAVES NO JOB BEHIND but it did take time, and time
    // is what the table is for: measuring only the detached ones would
    // count the wins and none of the calls they are a fraction of.
    let seen = readings(&s);
    assert_eq!(seen.len(), 2, "a reading went missing: {seen:?}");
    assert!(seen.iter().all(|r| r["project"].as_str().is_some()));
}

#[test]
fn the_table_never_holds_the_line_as_typed() {
    let s = Scratch::new("secret");
    // AN INLINE ASSIGNMENT IS WHERE A SECRET LIVES, and this table sits
    // in a cache directory for weeks. Truncating it would not do: a
    // truncated secret is still a leaked prefix, so it is dropped whole.
    s.run(&["run", "--", "TOKEN=hunter2 echo done"]);
    let raw = std::fs::read_to_string(s.home().join("readings.jsonl")).unwrap();
    assert!(!raw.contains("hunter2"), "the secret was written down: {raw}");
    assert!(!raw.contains("TOKEN"), "the assignment was kept: {raw}");
    assert!(raw.contains("echo done"), "the shape was lost with it: {raw}");
}

#[test]
fn a_shape_is_the_same_whichever_door_it_came_through() {
    let s = Scratch::new("shape");
    s.run(&["run", "--", "git status"]);
    s.run(&["run", "--", "rtk git status"]);
    let shapes: Vec<String> = readings(&s)
        .iter()
        .map(|r| r["shape"].as_str().unwrap_or("").to_string())
        .collect();
    // The hook adds `rtk`; a hand-typed line does not. One command must
    // not be filed under two shapes because of that.
    assert_eq!(shapes, vec!["git status", "git status"], "grouped by the door");
}

#[test]
fn waiting_on_a_job_is_not_counted_as_time_saved() {
    let s = Scratch::new("honest");
    // TWO IDENTICAL LINES, TREATED DIFFERENTLY. One is detached and left
    // alone; the other is detached and then waited on. The second saved
    // nobody anything, and a tool that counted both would be reporting
    // its own good intentions.
    s.run(&["run", "--after", "1", "--", "sleep 3"]);
    let said = text(&s.run(&["run", "--after", "1", "--", "sleep 3"]));
    let id = announced(&said);
    s.run(&["wait", &id]);

    let blocks: f64 = readings(&s)
        .iter()
        .filter(|r| r["kind"] == "wait")
        .filter_map(|r| r["secs"].as_f64())
        .sum();
    assert!(blocks > 1.0, "the block was not written down: {blocks}");

    let shown = text(&s.run(&["gain"]));
    // Elapsed is ~6s; one line gave back ~2s, the other gave back ~2s and
    // then took it straight back. So the headline must be nearer 2s than
    // 4s — the exact figure moves with the machine, the halving does not.
    let compressed: f64 = shown
        .lines()
        .find(|l| l.contains("saved — command time"))
        .and_then(|l| l.split_whitespace().next().map(|w| w.trim_end_matches('s').to_string()))
        .and_then(|w| w.parse().ok())
        .unwrap_or(-1.0);
    assert!(
        (0.5..3.4).contains(&compressed),
        "the wait was not subtracted — claimed {compressed}s from:\n{shown}"
    );
}

#[test]
fn stats_group_by_project() {
    let s = Scratch::new("byproject");
    s.run(&["run", "--", "echo x"]);
    let shown = text(&s.run(&["gain"]));
    assert!(shown.contains("project"), "no heading: {shown}");
    assert!(shown.contains("saved"), "the number the tool exists for is missing");
}

// ── BEING TOLD IT ENDED ─────────────────────────────────────────────────

#[test]
fn a_short_line_announces_nothing() {
    let s = Scratch::new("quiet");
    s.run_as("me", &["run", "--", "echo x"]);
    // A LINE THAT FINISHED IN TIME WAS NEVER A JOB. Announcing every
    // command would be a notification per shell call, which is how a
    // notification stops being read at all.
    assert!(text(&s.run_as("me", &["signals", "agent"])).trim().is_empty());
}

#[test]
fn a_detached_job_is_announced_once_and_only_once() {
    let s = Scratch::new("told");
    let said = text(&s.run_as("me", &["run", "--after", "1", "--", "sleep 2; exit 7"]));
    let id = announced(&said);
    ended(&s, &id);

    let first = text(&s.run_as("me", &["signals", "agent"]));
    assert!(first.contains(&id), "the ending never arrived: {first}");
    assert!(first.contains("exit=7"), "the code was lost: {first}");
    // READ AND ERASED IN ONE GESTURE. What makes each ending announced
    // exactly once is that nothing is left to announce again.
    let second = text(&s.run_as("me", &["signals", "agent"]));
    assert!(second.trim().is_empty(), "it spoke twice: {second}");
}

#[test]
fn the_two_audiences_do_not_take_each_others_endings() {
    let s = Scratch::new("audiences");
    let said = text(&s.run_as("me", &["run", "--after", "1", "--", "sleep 2"]));
    let id = announced(&said);
    ended(&s, &id);
    s.run_as("me", &["signals", "agent"]);
    // THE PERSON'S COPY SURVIVES THE MODEL READING ITS OWN. One human
    // wants every ending, whichever session started it.
    let human = text(&s.run_as("me", &["signals", "user"]));
    assert!(human.contains(&id), "the person's copy was taken too: {human}");
}

#[test]
fn stop_blocks_on_our_own_failure_and_not_on_somebody_elses() {
    let s = Scratch::new("blocking");
    let said = text(&s.run_as("them", &["run", "--after", "1", "--", "sleep 2; exit 3"]));
    let id = announced(&said);
    ended(&s, &id);

    // ANNOUNCING IS ONE THING, BLOCKING IS ANOTHER. Blocking holds a
    // session open and sends the model to fix something; doing that for
    // a job another session started sends an agent to read a log from a
    // project it is not working on. Measured the day it happened.
    let theirs = s.event("them", r#"{"hook_event_name":"Stop"}"#);
    let parsed: serde_json::Value = serde_json::from_str(theirs.trim()).unwrap();
    assert_eq!(parsed["decision"], "block", "our own failure did not hold us: {theirs}");

    let said = text(&s.run_as("them", &["run", "--after", "1", "--", "sleep 2; exit 3"]));
    let id = announced(&said);
    ended(&s, &id);
    let mine = s.event("me", r#"{"hook_event_name":"Stop"}"#);
    assert!(mine.trim().is_empty() || !mine.contains("block"),
            "somebody else's failure stopped us: {mine}");
}

// ── THE OTHER DOOR: WORK HANDED OVER BEFORE IT STARTS ───────────────────

#[test]
fn queue_holds_work_back_when_the_slots_are_full() {
    let s = Scratch::new("cap");
    let cap = [("JBX_SLOTS", "1")];
    let mut ids = Vec::new();
    for n in 1..=3 {
        let out = s.run_with(&cap, &["queue", &format!("job-{n}"), "--", "sleep 2"]);
        ids.push(text(&out).lines().next().unwrap_or("").trim().to_string());
    }
    // A CAP ONLY MEANS SOMETHING HERE. `run` wraps a command that is
    // already running, so holding it back would hold back nothing; this
    // takes work that has not started, and that can wait its turn.
    let shown = text(&s.run_with(&cap, &["list"]));
    let queued = shown.lines().filter(|l| l.contains("queued")).count();
    assert_eq!(queued, 2, "the cap did not hold anything back:\n{shown}");
    assert!(shown.contains("background"), "nothing started at all:\n{shown}");

    // AND A DELIBERATE JOB IS ANNOUNCED WHATEVER ITS DURATION. Somebody
    // chose to hand it over; a two-second one they chose to hand over is
    // still an ending they are waiting for.
    //
    // READ BEFORE WAITING, and the order is the assertion: `jbx wait`
    // now clears the ending it delivers, so collecting the mail first is
    // the only way to ask whether it was ever posted.
    for id in &ids {
        ended(&s, id);
    }
    let told = text(&s.run_as("me", &["signals", "agent"]));
    for id in &ids {
        assert!(told.contains(id), "{id} was never announced:\n{told}");
    }
    // The record outlives the ending, so the exit code is still there to
    // be had — which is what says the cap ran them rather than lost them.
    for id in &ids {
        assert_eq!(s.run_with(&cap, &["wait", id]).status.code(), Some(0));
    }
}

#[test]
fn queue_refuses_to_run_something_nobody_named() {
    let s = Scratch::new("named");
    // THE INTENT IS MANDATORY HERE AND NOWHERE ELSE. `run` names a line
    // after the fact because nobody chose to background it; somebody
    // choosing to has a name in mind.
    let out = s.run(&["queue", "--", "sleep 1"]);
    assert_eq!(out.status.code(), Some(2), "it queued an unnamed job");
}

#[test]
fn health_names_a_job_that_runs_without_saying_anything() {
    let s = Scratch::new("mute");
    let quiet = [("JBX_MUTE_AFTER", "1")];
    // TWO SILENCES THAT ARE NOT THE SAME. One job has not written a byte;
    // the other said one line and went quiet. By the freshness of the log
    // alone they looked identical, and a testbox worker whose output went
    // through `| tail` was read as dead for forty minutes while it worked.
    let never = announced(&text(&s.run_with(&quiet, &["run", "--after", "1", "--", "sleep 6"])));
    let quieted = announced(&text(&s.run_with(&quiet, &["run", "--after", "1", "--", "echo started; sleep 6"])));
    std::thread::sleep(std::time::Duration::from_secs(2));

    let out = s.run_with(&quiet, &["health"]);
    let shown = text(&out);
    // RUNNING IS NOT MAKING PROGRESS, and the two look identical from
    // outside. A job that says nothing is NAMED rather than counted — a
    // number here would send the reader to `list` to find out which one —
    // and it is named under the section that says which silence it is.
    let mut section = "";
    let mut under = std::collections::HashMap::new();
    for line in shown.lines() {
        if line.trim_start().starts_with("NO OUTPUT —") {
            section = "NO OUTPUT";
        } else if line.trim_start().starts_with("MUTE —") {
            section = "MUTE";
        }
        for id in [&never, &quieted] {
            if line.contains(id.as_str()) {
                under.insert(id.clone(), section);
            }
        }
    }
    assert_eq!(under.get(&never).copied(), Some("NO OUTPUT"),
               "a job that never wrote was not said to have no output:\n{shown}");
    assert_eq!(under.get(&quieted).copied(), Some("MUTE"),
               "a job that wrote and went quiet was not said to be mute:\n{shown}");
    assert_eq!(out.status.code(), Some(1), "health said all is well");

    // AND THE LISTING SAYS THE SAME, in the column people actually read.
    let listed = text(&s.run_with(&quiet, &["list", "--width", "200"]));
    let row = |id: &str| listed.lines().find(|l| l.starts_with(id)).unwrap_or("").to_string();
    assert!(row(&never).contains("NO OUTPUT"), "`list` called it something else:\n{listed}");
    assert!(row(&quieted).contains("MUTE"), "`list` called it something else:\n{listed}");
    s.run(&["kill", &never]);
    s.run(&["kill", &quieted]);
}

#[test]
fn config_says_where_each_value_came_from() {
    let s = Scratch::new("config");
    let shown = text(&s.run_with(&[("JBX_AFTER", "5")], &["config"]));
    // THE SECOND COLUMN IS THE POINT. A value alone invites the reader
    // to guess whether it is theirs or a default, and the day those two
    // disagree is the day the question matters.
    let line = shown.lines().find(|l| l.starts_with("  after")).unwrap_or("");
    assert!(line.contains("environment"), "a set value looked like a default: {line}");
    let line = shown.lines().find(|l| l.contains("mute_after")).unwrap_or("");
    assert!(line.contains("default"), "an unset value looked like a choice: {line}");
    // AND WHERE TO EDIT, even when the file is not there yet: a reader
    // who wants to change something needs the path more than they need
    // to be told it does not exist.
    assert!(shown.contains("global config"), "it did not say where to edit: {shown}");
}

#[test]
#[cfg(unix)]
fn a_closed_pipe_is_not_a_crash() {
    let s = Scratch::new("pipe");
    // `jbx list | head` CLOSES THE PIPE ON PURPOSE, and Rust ignores
    // SIGPIPE at startup — so a plain `println!` panics with a stack
    // trace where every other Unix tool simply stops. The Python this
    // replaces handled it; the rewrite lost it, and only piping into
    // `head` by hand showed that.
    let out = Command::new("sh")
        .env_remove("JBX_WRAPPED")
        .arg("-c")
        .arg(format!("{} config | head -1", jbx_in_line()))
        .env("JBX_DIR", &s.0)
        .output()
        .unwrap();
    let noise = String::from_utf8_lossy(&out.stderr);
    assert!(!noise.contains("panicked"), "it panicked on a closed pipe: {noise}");
    assert_eq!(out.status.code(), Some(0), "a closed pipe was treated as a failure");
}

// ── SAYING WHERE jbx APPLIES, AND HOW ───────────────────────────────────

fn ask_hook(binary_dir: &std::path::Path, s: &Scratch, global: &str) -> String {
    use std::io::Write;
    let config = s.0.join("global.yaml");
    std::fs::write(&config, global).unwrap();
    let event = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "git status", "description": "", "timeout": 1},
    })
    .to_string();
    let mut child = Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .arg("hook")
        .current_dir(binary_dir)
        .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", &config)
        .env("PATH", "/nonexistent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(event.as_bytes()).unwrap();
    String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).into_owned()
}

#[test]
fn jbx_applies_everywhere_until_a_project_says_otherwise() {
    let s = Scratch::new("everywhere");
    // THE DEFAULT IS EVERYWHERE, and that is the design: a list of
    // projects worth wrapping would be the prediction this tool refuses.
    let plain = s.project(None, "src");
    assert!(ask_hook(&plain, &s, "").contains("run --"), "it did not wrap by default");
}

#[test]
fn a_project_can_switch_jbx_off_entirely() {
    let s = Scratch::new("off");
    let here = s.project(Some("enabled: false\n"), "src/deep");
    // OFF MEANS OFF: not a longer threshold, not a quieter mode. The
    // hook says nothing at all, so commands run exactly as they would
    // with jbx uninstalled — the only promise worth making to somebody
    // who asked it to stay out of the way.
    assert!(ask_hook(&here, &s, "").trim().is_empty(), "it still spoke");
    // AND IT IS FOUND FROM A SUBDIRECTORY. A setting that stopped
    // applying two directories down would be a setting nobody could rely
    // on.
    assert!(here.ends_with("deep"));
}

#[test]
fn a_project_overrides_the_global_file_key_by_key() {
    let s = Scratch::new("layers");
    let here = s.project(Some("after: 5\n"), "");
    let shown = text(&s.run_in(&here, "after: 99\nmute_after: 42\n", &["config"]));
    let after = shown.lines().find(|l| l.starts_with("  after")).unwrap_or("");
    let mute = shown.lines().find(|l| l.contains("mute_after")).unwrap_or("");
    assert!(after.contains("5s") && after.contains("this project"), "project lost: {after}");
    // KEY BY KEY. A project naming one setting must not silence every
    // other setting the global file made — that is an afternoon lost to
    // a file that looks right.
    assert!(mute.contains("42s") && mute.contains("global"), "global was silenced: {mute}");
}

// ── ASKING FOR THE FOREGROUND ON PURPOSE ────────────────────────────────

#[test]
fn fg_never_lets_go_however_long_it_takes() {
    let s = Scratch::new("fg");
    // THE DELIBERATE FOREGROUND. `--after 1` would detach anything else;
    // this is the caller saying they need the answer before they can go
    // on, so the threshold does not apply to them.
    let out = s.run_with(&[("JBX_AFTER", "1")], &["fg", "--", "sleep 3; exit 4"]);
    assert!(!text(&out).contains("detached"), "it let go of a deliberate foreground");
    assert_eq!(out.status.code(), Some(4), "the code did not come straight back");
}

#[test]
fn fg_brings_a_detached_job_back() {
    let s = Scratch::new("attach");
    let said = text(&s.run(&["run", "--after", "1", "--", "echo early; sleep 2; exit 6"]));
    let id = announced(&said);

    let out = s.run(&["fg", &id]);
    let shown = text(&out);
    // FROM THE BEGINNING, not from where we happened to arrive: the
    // point of picking it back up is seeing what it did, and half of
    // that already happened.
    assert!(shown.contains("early"), "the log so far was lost: {shown}");
    assert_eq!(out.status.code(), Some(6), "the real code did not come back");
}

#[test]
fn fg_tells_an_id_from_a_command() {
    let s = Scratch::new("either");
    // An id is `j` and seven hex digits and nothing else, which no
    // command is — so a word that only looks close falls through to
    // "run this line" rather than to somebody else's job.
    let out = s.run(&["fg", "--", "echo jdeadbee"]);
    assert_eq!(text(&out).trim(), "jdeadbee", "it took a word for an id");
    let out = s.run(&["fg", "jdeadbee"]);
    assert_eq!(out.status.code(), Some(1), "an unknown id was run as a command");
}

// ── THE THINGS THAT DRIFT ───────────────────────────────────────────────

#[test]
fn the_version_matches_the_changelog() {
    // TWO PLACES HOLDING ONE NUMBER IS EXACTLY HOW THEY COME TO
    // DISAGREE, and the day they do is a release whose notes describe
    // something else. The changelog is the source; `Cargo.toml` follows.
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    let declared = manifest
        .lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|l| l.split('"').next())
        .expect("Cargo.toml declares a version");

    let changelog = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/CHANGELOG.md")).unwrap();
    let newest = changelog
        .lines()
        .find_map(|l| l.strip_prefix("## ["))
        .and_then(|l| l.split(']').next())
        .expect("the changelog has a release");

    assert_eq!(
        declared, newest,
        "Cargo.toml says {declared} and the changelog's newest entry is {newest}"
    );
}

#[test]
fn every_verb_in_the_readme_exists() {
    // A README NAMING A VERB THAT WAS RENAMED is the first thing a new
    // reader tries, and the failure they meet is `unknown verb`. The
    // help text is what the binary really answers to, so the two are
    // compared rather than trusted.
    // BOTH DOCUMENTS, because the verbs moved. The README is the pitch
    // and USAGE.md is the reference; a verb named in either is named,
    // and a verb named in neither is the failure this guard exists for.
    // AND `CLI-AI.md`, which names verbs in fenced blocks like the other
    // two. A document left out of this list drifts silently, and this one
    // is the page somebody follows while wiring a client by hand.
    let readme = ["/README.md", "/USAGE.md", "/CLI-AI.md"]
        .iter()
        .map(|f| {
            std::fs::read_to_string(format!("{}{f}", env!("CARGO_MANIFEST_DIR"))).unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let help = text(&Command::new(JBX)
        .env_remove("JBX_WRAPPED").arg("--help").output().unwrap());
    let mut missing = Vec::new();
    // INSIDE FENCED BLOCKS ONLY. Prose says "jbx removes the question",
    // and reading that as a verb makes the guard cry wolf — which is how
    // a guard stops being read.
    let mut fenced = false;
    for line in readme.lines() {
        if line.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if !fenced {
            continue;
        }
        if let Some(rest) = line.strip_prefix("jbx ") {
            let verb = rest.split_whitespace().next().unwrap_or("");
            if !verb.is_empty() && !help.contains(&format!("jbx {verb}")) {
                missing.push(verb.to_string());
            }
        }
    }
    assert!(missing.is_empty(), "the README names verbs the binary does not have: {missing:?}");

    // AND THE OTHER DIRECTION, which is the one that goes unnoticed: a
    // guard that only checks the README names nothing false never
    // notices it naming nothing at all. `jbx hook` was missing for six
    // versions, and it is the verb `init` writes into a settings file
    // people then read.
    let mut unlisted = Vec::new();
    for line in help.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("jbx ") {
            let verb = rest.split_whitespace().next().unwrap_or("");
            let named = verb.chars().all(|c| c.is_ascii_lowercase() || c == '-');
            if named && !verb.is_empty() && !readme.contains(&format!("jbx {verb}")) {
                unlisted.push(verb.to_string());
            }
        }
    }
    assert!(unlisted.is_empty(), "the binary has verbs the README never mentions: {unlisted:?}");
}

#[test]
fn a_pipeline_stage_is_not_reported_as_stuck() {
    let s = Scratch::new("pipeline");
    // #2063, IN ONE LINE. `cat` is blocked reading the pipe for the
    // whole five seconds and the command finishes with 0 — yet this used
    // to announce "it will not finish on its own" and advise killing it.
    // Being stopped in `read(0)` says the process is reading its input,
    // and a pipeline stage waiting on a slow producer is exactly that.
    let said = text(&s.run(&["run", "--after", "1", "--", "sleep 3 | cat; echo done"]));
    assert!(said.contains("BACKGROUND as j"), "not detached at all: {said}");
    assert!(
        !said.contains("reading its standard input"),
        "an ordinary pipeline was called stuck:\n{said}"
    );
    // AND NOTHING IN IT PREDICTS. The costly half of the old message was
    // not the guess but the certainty: "it will not finish on its own",
    // about a deployment that had.
    assert!(!said.contains("will not finish"), "it still predicts:\n{said}");

    let id = announced(&said);
    assert_eq!(s.run(&["wait", &id]).status.code(), Some(0), "and it did finish");
}

#[test]
#[cfg(unix)]
fn the_line_goes_to_the_shell_it_was_written_for() {
    let s = Scratch::new("shell");
    // A SHELL OF OUR OWN, so the test observes which one was used rather
    // than inferring it from output that several shells would produce.
    let fake = s.0.join("say-which");
    std::fs::write(&fake, "#!/bin/sh\necho \"ran by say-which: $2\"\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

    let out = s.run_with(&[("JBX_SHELL", fake.to_str().unwrap())], &["run", "--", "echo hello"]);
    // THE HOOK QUOTES FOR A POSIX SHELL, so the runner has to be one —
    // on Windows those two halves used to disagree, and nobody had run
    // it there to find out.
    assert!(
        text(&out).contains("ran by say-which: echo hello"),
        "the named shell did not run the line: {}",
        text(&out)
    );
}

// ── PROJECTS THAT SHARE A NAME, AND PROJECTS INSIDE PROJECTS ────────────

/// Run one command from a directory made to look like a project.
fn run_from(s: &Scratch, dir: &std::path::Path, line: &str) {
    std::fs::create_dir_all(dir.join(".claude")).unwrap();
    Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .args(["run", "--", line])
        .current_dir(dir)
        .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
}

#[test]
fn two_projects_sharing_a_name_are_not_one_project() {
    let s = Scratch::new("homonyms");
    run_from(&s, &s.0.join("alpha/api"), "echo a");
    run_from(&s, &s.0.join("beta/api"), "echo b");

    let shown = text(&s.run(&["gain"]));
    let rows: Vec<&str> = shown.lines().filter(|l| l.contains("api")).collect();
    // SUMMING THEM MADE ONE ROW whose every number was the total of two
    // unrelated things. Two directories called `api` are two projects.
    assert_eq!(rows.len(), 2, "the two `api` were merged:\n{shown}");
    // AND THEY MUST BE TELLABLE APART. A row you cannot name is a row you
    // cannot ask about.
    assert_ne!(rows[0].trim(), rows[1].trim(), "nothing distinguishes them:\n{shown}");
}

#[test]
fn a_project_inside_a_project_is_shown_inside_it() {
    let s = Scratch::new("nested");
    let outer = s.0.join("outer");
    run_from(&s, &outer, "echo outer");
    run_from(&s, &outer.join("tool"), "echo inner");

    let shown = text(&s.run(&["gain"]));
    let outer_line = shown.lines().position(|l| l.contains("outer")).unwrap();
    let inner_line = shown.lines().position(|l| l.trim_start().starts_with("tool")).unwrap();
    // A REPOSITORY INSIDE A REPOSITORY IS THE ORDINARY CASE — a tool
    // living in the tree of the thing it serves. A flat list hides it
    // exactly where it matters.
    assert!(inner_line > outer_line, "the child was not under its parent:\n{shown}");
    let inner = shown.lines().nth(inner_line).unwrap();
    assert!(inner.starts_with("  "), "the child was not indented: {inner:?}");
    // AND THE CHILD IS NAMED BY WHAT IT IS, not by the whole road to it.
    assert!(!inner.contains("outer/tool"), "the full path leaked in: {inner:?}");
}

#[test]
fn project_path_shows_the_road_when_asked() {
    let s = Scratch::new("paths");
    run_from(&s, &s.0.join("here"), "echo x");
    let shown = text(&s.run(&["gain", "--project-path"]));
    assert!(shown.contains(&s.0.join("here").display().to_string()),
            "the full path was not shown:\n{shown}");
}

#[test]
#[cfg(unix)]
fn a_reader_that_leaves_early_is_told_its_view_was_partial() {
    let s = Scratch::new("mirror");
    // #2066: what jbx prints is a MIRROR of the job's log. Closing it
    // early truncates what you SEE, never what runs — and the truncated
    // mirror reads exactly like the whole story. Somebody concluded a
    // suite had finished, re-ran it, and the two collided.
    //
    // THE CONDITION IS "THE READER LEFT", NOT "IT IS A PIPE". A first
    // attempt warned on any pipe, which fires on `x=$(jbx run …)` —
    // an ordinary capture that reads to the end and misses nothing. A
    // write that FAILS is the exact fact, and it has no false positive.
    let out = Command::new("sh")
        .env_remove("JBX_WRAPPED")
        .arg("-c")
        .arg(format!("{} run -- 'seq 20000' | head -3", jbx_in_line()))
        .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("stopped early"), "a cut mirror went unmentioned: {said}");

    // AND AN ORDINARY CAPTURE IS SILENT. `.output()` reads everything,
    // which is what a caller collecting the result does.
    let quiet = s.run(&["run", "--", "echo hi"]);
    let noise = String::from_utf8_lossy(&quiet.stderr);
    assert!(!noise.contains("stopped early"), "it warned at a reader that stayed: {noise}");
}

#[test]
fn a_wrapped_line_that_runs_jbx_makes_one_job_not_two() {
    let s = Scratch::new("inner-run");
    // #2066, second symptom: the hook wraps every command, so when the
    // command it wrapped is itself a `jbx run` there were TWO jobs. The
    // id announced was the OUTER one, which ends in seconds with exit 0
    // and a log holding nothing but the inner's detachment message —
    // reading exactly like a finished job while the real one runs on
    // under an id nobody was told. Four wrong ids in one session.
    let said = text(&s.run(&[
        "run", "--after", "1", "--",
        &format!("{} run --after 3 -- 'sleep 6; echo REAL'", jbx_in_line()),
    ]));
    let id = announced(&said);

    let listed = text(&s.run(&["list"]));
    let jobs = listed.lines().skip(1).filter(|l| l.trim_start().starts_with('j')).count();
    assert_eq!(jobs, 1, "the inner run made a second job:\n{listed}");

    // AND THE ID ANNOUNCED IS THE ONE DOING THE WORK — which is the
    // whole point: an id you cannot trust is worse than no id.
    assert_eq!(s.run(&["wait", &id]).status.code(), Some(0));
    assert!(text(&s.run(&["tail", &id])).contains("REAL"),
            "the announced id was not the one carrying the work");
}

#[test]
fn a_reading_belongs_to_the_calling_session_not_to_wherever_it_ran() {
    let s = Scratch::new("session-root");
    let home = s.project(None, "");
    // What the hook writes down the first time it sees a session: the
    // directory of the Claude Code that is calling.
    let roots = s.0.join("cache/sessions");
    std::fs::create_dir_all(&roots).unwrap();
    std::fs::write(roots.join("abcd1234"), home.display().to_string()).unwrap();

    // A COMMAND RUN SOMEWHERE ELSE ENTIRELY. A session's working
    // directory moves — one `cd` moves it for every command after — so
    // filing by it splits one session's time across whatever it walked
    // through: measured on a real store, a row froze at the minute a
    // session stepped into a sub-project and a second row started.
    let elsewhere = s.0.join("far/away");
    std::fs::create_dir_all(&elsewhere).unwrap();
    Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .args(["run", "--", "echo x"])
        .current_dir(&elsewhere)
        .env("JBX_DIR", &s.0)
        .env("JBX_CONFIG", s.0.join("global.yaml"))
        .env("CLAUDE_CODE_SESSION_ID", "abcd1234")
        .stdin(Stdio::null())
        .output()
        .unwrap();

    let where_filed = readings(&s)
        .last()
        .and_then(|r| r["path"].as_str().map(str::to_string))
        .unwrap_or_default();
    assert_eq!(
        where_filed,
        home.display().to_string(),
        "the reading followed the working directory instead of the session"
    );
}

#[test]
fn without_a_session_it_falls_back_to_where_it_stands() {
    let s = Scratch::new("no-session");
    let here = s.project(None, "");
    // A PLAIN SHELL HAS NO SESSION AND NO HOOK to have written one down.
    // Refusing to file anything would lose the reading; walking up from
    // here is the honest answer, and it is what a person in a terminal
    // means anyway.
    Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .args(["run", "--", "echo x"])
        .current_dir(&here)
        .env("JBX_DIR", &s.0)
        .env("JBX_CONFIG", s.0.join("global.yaml"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let filed = readings(&s).last().and_then(|r| r["path"].as_str().map(str::to_string)).unwrap_or_default();
    // CANONICAL BOTH SIDES. On macOS the scratch lives under
    // `/var/folders/…`, which is a link to `/private/var/…` — two
    // spellings of one directory, and the reading is filed under the
    // one the kernel answers with.
    let filed = std::fs::canonicalize(&filed).unwrap_or_else(|_| filed.into());
    let here = std::fs::canonicalize(&here).unwrap_or(here);
    assert_eq!(filed, here, "it did not fall back to the cwd");
}

#[test]
fn global_only_leaves_the_project_alone() {
    let s = Scratch::new("global-only");
    let config = s.0.join("claude");
    std::fs::create_dir_all(&config).unwrap();
    // A PROJECT, by the marker `init` looks for.
    let here = s.project(None, "");
    std::fs::create_dir_all(here.join(".git")).unwrap();

    let run = |flags: &[&str]| {
        Command::new(JBX)
            .env_remove("JBX_WRAPPED")
            .arg("init")
            .args(flags)
            .current_dir(&here)
            .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
            .env("CLAUDE_CONFIG_DIR", &config)
            .output()
            .unwrap()
    };

    // THE INSTALLER RUNS FROM WHEREVER SOMEBODY WAS STANDING, which may
    // well be inside a repository — and a file appearing in your project
    // because you installed a tool is a surprise, however commented it
    // is. The hooks are still declared: that is the whole point of
    // running it.
    run(&["--global-only"]);
    assert!(!here.join(".jbx.yaml").exists(),
            "`--global-only` wrote a project file anyway");
    let settings = config.join("settings.json");
    assert!(settings.exists(), "`--global-only` declared no hooks at all");
    let declared: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert!(declared["hooks"]["PreToolUse"].is_array(), "no PreToolUse hook: {declared}");

    // AND WITHOUT THE FLAG IT STILL WRITES ONE, which is the half that
    // makes the assertion above mean something.
    run(&[]);
    assert!(here.join(".jbx.yaml").exists(), "the project file stopped being written");
}

#[test]
fn the_declared_hook_survives_the_shell_that_runs_it() {
    let s = Scratch::new("hook-spelling");
    let config = s.0.join("claude");
    std::fs::create_dir_all(&config).unwrap();
    let here = s.project(None, "");
    Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .arg("init")
        .current_dir(&here)
        .env("JBX_DIR", &s.0)
        .env("JBX_CONFIG", s.0.join("global.yaml"))
        .env("CLAUDE_CONFIG_DIR", &config)
        .output()
        .unwrap();
    let settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(config.join("settings.json")).unwrap()).unwrap();
    let declared = settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str().unwrap();

    // THE HARNESS HANDS THIS LINE TO A SHELL. IT DOES NOT EXEC IT.
    //
    // So the question is not "is the path correct" -- it was, as a path
    // -- but "does a shell still find it afterwards". MEASURED on
    // Windows, from an install that had just reported success: declared
    // with backslashes, the line reached bash as
    // `C:UsersberliAppDataLocaljbxbinjbx.exe: command not found`, on
    // every prompt of every session, and nothing in the install said so.
    let out = Command::new("sh")
        .arg("-c")
        .arg(declared)
        .env_remove("JBX_WRAPPED")
        .env("JBX_DIR", &s.0)
        .env("JBX_CONFIG", s.0.join("global.yaml"))
        .stdin(Stdio::null())
        .output()
        .expect("a shell runs");
    assert!(
        out.status.success(),
        "a shell could not run the declared hook:\n  {declared}\n  {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
#[cfg(unix)]
fn init_declares_the_link_it_was_called_through() {
    let s = Scratch::new("through-a-link");
    let config = s.0.join("claude");
    std::fs::create_dir_all(&config).unwrap();
    let here = s.project(None, "");
    let link = s.0.join("jbx");
    std::os::unix::fs::symlink(JBX, &link).unwrap();

    let run = |program: &std::path::Path| {
        Command::new(program)
            .env_remove("JBX_WRAPPED")
            .arg("init")
            .current_dir(&here)
            .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
            .env("CLAUDE_CONFIG_DIR", &config)
            .output()
            .unwrap()
    };
    let declared = || -> String {
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(config.join("settings.json")).unwrap()).unwrap();
        v["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str().unwrap().to_string()
    };

    // THROUGH A LINK, THE LINK IS WHAT IS DECLARED. `current_exe()`
    // follows symlinks, so a dev install nailed the hook to the build
    // tree — right in that a rebuild is picked up, wrong in that moving
    // the tree breaks every session at once.
    run(&link);
    assert!(declared().starts_with(link.to_str().unwrap()),
            "the link was resolved away: {}", declared());

    // AND RE-RUNNING BRINGS AN OLD DECLARATION UP TO DATE rather than
    // shrugging. "already declared" used to mean "left pointing at
    // wherever it pointed before", which reads like nothing to do.
    let out = run(std::path::Path::new(JBX));
    assert!(text(&out).contains("repointed"), "it did not correct the path: {}", text(&out));
    assert!(declared().starts_with(JBX), "still the old path: {}", declared());
}

// ── WHAT IS HAPPENING, AND WHO IS HOLDING IT ────────────────────────────

#[test]
fn a_held_line_and_a_let_go_one_do_not_read_alike() {
    let s = Scratch::new("held");
    // Held: the launcher is still there, the output is still mirroring
    // to whoever asked, and the line may yet finish in time and leave
    // nothing behind. Let go of: only the log receives anything.
    // `running` said neither.
    let long = std::process::Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .args(["run", "--after", "30", "--", "sleep 4"])
        .env("JBX_DIR", &s.0)
        .env("JBX_CONFIG", s.0.join("global.yaml"))
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    s.run(&["run", "--after", "1", "--", "sleep 4"]);
    std::thread::sleep(std::time::Duration::from_millis(1500));

    let shown = text(&s.run(&["list"]));
    assert!(shown.contains("foreground"), "a held line did not say so:\n{shown}");
    assert!(shown.contains("background"), "a let-go line did not say so:\n{shown}");

    let mut long = long;
    let _ = long.kill();
    let _ = long.wait();
}

#[test]
fn ps_shows_what_is_happening_and_list_shows_the_day() {
    let s = Scratch::new("ps");
    s.run(&["run", "--", "echo done"]);
    s.run(&["run", "--after", "1", "--", "sleep 4"]);
    std::thread::sleep(std::time::Duration::from_millis(1500));

    // "What is going on right now" is asked far more often than "what
    // went on today", and a day of finished jobs between you and the
    // answer is a list you stop reading.
    let running = text(&s.run(&["ps"]));
    assert!(running.contains("sleep 4"), "ps lost the running job:\n{running}");
    assert!(!running.contains("exit 0"), "ps showed a finished job:\n{running}");
}

#[test]
fn stopping_a_job_before_it_starts_leaves_a_state_that_says_so() {
    let s = Scratch::new("cancel-queued");
    let one = [("JBX_SLOTS", "1")];
    for n in 1..=3 {
        s.run_with(&one, &["queue", &format!("j{n}"), "--", "sleep 3"]);
    }
    std::thread::sleep(std::time::Duration::from_millis(800));
    let listed = text(&s.run_with(&one, &["list"]));
    let victim = listed
        .lines()
        .find(|l| l.contains("queued"))
        .and_then(|l| l.split_whitespace().next())
        .expect("something is waiting its turn")
        .to_string();

    s.run_with(&one, &["kill", &victim]);
    std::thread::sleep(std::time::Duration::from_millis(500));

    // STOPPING WORK THAT HAS NOT STARTED IS LEGITIMATE, and the state it
    // leaves has to say so. The queued branch answered before the
    // liveness check, so a cancelled job read "waiting for a slot" for
    // ever — and `wait` on it blocked for ever with it.
    let after = text(&s.run_with(&one, &["status", &victim]));
    assert!(!after.contains("waiting for a slot"), "it still claims to be waiting:\n{after}");
    assert_eq!(s.run_with(&one, &["wait", &victim]).status.code(), Some(1),
               "`wait` did not come back");
    // AND NOTHING LEFT WAITING when the scratch directory goes: queued jobs
    // outliving their store is how this suite left orphans on a machine.
    s.stop_everything();
}

#[test]
fn queue_says_out_loud_when_a_job_does_not_start() {
    let s = Scratch::new("stacked");
    let one = [("JBX_SLOTS", "1")];
    let first = text(&s.run_with(&one, &["queue", "a", "--", "sleep 3"]));
    // WAIT FOR THE SLOT TO BE HELD, don't assume it. `queue` returns as
    // soon as the record is written; the supervisor takes the lock a
    // moment later. This raced on a loaded macOS runner — the second
    // job was filed before the first had claimed anything, so it was
    // told a slot was free, which was true at that instant and useless.
    until("the first job holds its slot", || {
        text(&s.run_with(&one, &["ps"])).contains("sleep 3")
    });
    let second = text(&s.run_with(&one, &["queue", "b", "--", "sleep 3"]));

    // A VERB THAT ANSWERS WITH AN ID AND NOTHING ELSE lets somebody
    // believe the work has begun. A job held back by a full queue looks
    // exactly like one already running, until they go and look.
    assert!(second.contains("NOT STARTED"), "the second one kept quiet:\n{second}");
    assert!(!first.contains("NOT STARTED"), "the first one claimed to be held:\n{first}");
    // AND THE ID IS STILL THE FIRST LINE, ALONE, because that is what a
    // script reads; the rest is for a person.
    assert!(first.lines().next().unwrap().trim().starts_with('j'));
    // AND THE SECOND ONE IS NOT LEFT WAITING for a store that is about to go.
    s.stop_everything();
}

#[test]
fn a_list_shows_this_project_and_counts_what_it_hides() {
    let s = Scratch::new("scoped");
    let mine = s.project(None, "");
    let other = s.0.join("elsewhere");
    std::fs::create_dir_all(other.join(".claude")).unwrap();

    let start = |dir: &std::path::Path| {
        Command::new(JBX)
            .env_remove("JBX_WRAPPED")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .args(["run", "--after", "1", "--", "sleep 4"])
            .current_dir(dir)
            .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
            .stdout(Stdio::null())
            .output()
            .unwrap();
    };
    start(&mine);
    start(&other);

    let here = text(&Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .arg("ps")
        .current_dir(&mine)
        .env("JBX_DIR", &s.0)
        .env("JBX_CONFIG", s.0.join("global.yaml"))
        .output()
        .unwrap());
    // ONE ROW, AND THE OTHER COUNTED. Hiding another project's work
    // makes a busy machine look idle, and somebody spends ten minutes
    // wondering why their own job never starts.
    assert_eq!(here.lines().filter(|l| l.trim_start().starts_with('j')).count(), 1,
               "the scope did not hold:\n{here}");
    assert!(here.contains("other projects"), "what was hidden went unsaid:\n{here}");
}

#[test]
#[cfg(unix)]
fn the_head_of_the_line_goes_first_even_when_slots_are_free() {
    let s = Scratch::new("ticket");
    // A TICKET HELD BY SOMEBODY ELSE, and slots to spare. Without an
    // order this job would start at once — "whoever asks when a slot is
    // free" was the rule, and it followed the filing order only because
    // waiters happen to start asking in that order.
    let tickets = s.0.join("cache/slots/tickets");
    std::fs::create_dir_all(&tickets).unwrap();
    let holder = Command::new("sleep").arg("20").spawn().unwrap();
    std::fs::write(tickets.join(format!("1.{}", holder.id())), "").unwrap();

    let out = s.run_with(&[("JBX_SLOTS", "4")], &["queue", "behind", "--", "echo RAN"]);
    let id = text(&out).lines().next().unwrap().trim().to_string();
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let waiting = text(&s.run_with(&[("JBX_SLOTS", "4")], &["status", &id]));
    assert!(waiting.contains("waiting for a slot"),
            "it went ahead of an older ticket with slots to spare:\n{waiting}");

    // AND A DEAD HOLDER MUST NOT BLOCK THE LINE FOR EVER — that is the
    // one way an ordered queue does worse than an unordered one.
    let mut holder = holder;
    let _ = holder.kill();
    let _ = holder.wait();
    assert_eq!(s.run_with(&[("JBX_SLOTS", "4")], &["wait", &id]).status.code(), Some(0),
               "the line never recovered from a dead ticket");
    assert!(text(&s.run_with(&[("JBX_SLOTS", "4")], &["tail", &id])).contains("RAN"));
}

// ── SAYING WHAT WE ARE, TO A MACHINE ────────────────────────────────────

#[test]
fn every_declared_verb_refuses_a_flag_it_does_not_take() {
    let s = Scratch::new("refuses");
    let doc: serde_json::Value =
        serde_json::from_str(text(&s.run(&["describe"])).trim()).expect("valid JSON");
    for c in doc["commands"].as_array().unwrap() {
        let verb = c["name"].as_str().unwrap();
        // `hook` answers a harness on stdin and takes no flags at all;
        // everything else parses BEFORE it acts, so this cannot start a
        // job or edit a setting on the way to being refused.
        if verb == "hook" {
            continue;
        }
        let out = s.run(&[verb, "--pleinecran"]);
        assert_eq!(out.status.code(), Some(2),
                   "`jbx {verb}` took a flag it never declared:\n{}", text(&out));
    }
}

#[test]
fn every_verb_that_declares_json_speaks_it() {
    let s = Scratch::new("speaks");
    // A JOB TO ASK ABOUT, so `status` has something real to answer.
    s.run(&["run", "--after", "1", "--intent", "measure the index", "--", "sleep 30"]);
    // NO SLEEP AFTER AN `until`. The poll above already waited for the
    // exact thing the next line needs, and a fixed pause beside it is a
    // guess about machine speed reintroduced into a test that had just
    // been rid of one.
    until("the named job is listed as running", || {
        text(&s.run(&["ps"])).contains("measure the index")
    });
    let id = text(&s.run(&["list", "--json"]));
    let id: serde_json::Value = serde_json::from_str(id.trim()).expect("valid JSON");
    let id = id[0]["id"].as_str().expect("a job").to_string();

    let doc: serde_json::Value =
        serde_json::from_str(text(&s.run(&["describe"])).trim()).expect("valid JSON");
    let mut asked = 0;
    for c in doc["commands"].as_array().unwrap() {
        let verb = c["name"].as_str().unwrap();
        let takes_json = c["options"]
            .as_array()
            .map(|o| o.iter().any(|f| f["name"] == "--json"))
            .unwrap_or(false);
        if !takes_json {
            continue;
        }
        // WHAT EACH ONE NEEDS BESIDES THE FLAG. A verb that wants an
        // argument and is given none answers a usage error, which is
        // not JSON and would say nothing about this rule.
        let mut args: Vec<&str> = vec![verb];
        match verb {
            "status" => args.push(&id),
            "signals" => args.push("agent"),
            _ => {}
        }
        args.push("--json");
        let out = text(&s.run(&args));
        // `signals` empties a mailbox and answers nothing when it is
        // already empty; nothing is not invalid.
        if out.trim().is_empty() {
            continue;
        }
        serde_json::from_str::<serde_json::Value>(out.trim())
            .unwrap_or_else(|e| panic!("`jbx {verb} --json` did not answer JSON ({e}):\n{out}"));
        asked += 1;
    }
    // AND THE COUNT IS PART OF THE TEST. A document that stopped
    // declaring `--json` anywhere would pass an empty loop in silence —
    // which is the exact shape of the bug this whole rule exists for.
    assert!(asked >= 6, "only {asked} verbs were checked; the document lost its flags");
    s.stop_everything();
}

#[test]
fn stats_can_be_asked_for_a_window_and_never_colours_a_pipe() {
    let s = Scratch::new("windows");
    s.run(&["run", "--", "echo counted"]);

    // A WINDOW IS A FILTER, and `all` is the one that filters nothing.
    let calls = |args: &[&str]| -> u64 {
        let mut all = vec!["gain", "--json"];
        all.extend_from_slice(args);
        let out = text(&s.run(&all));
        let v: serde_json::Value = serde_json::from_str(out.trim()).expect("valid JSON");
        v["total"]["calls"].as_u64().unwrap_or(0)
    };
    assert_eq!(calls(&["--since", "1h"]), calls(&["--since", "all"]),
               "a reading taken a second ago fell outside the last hour");
    assert_eq!(calls(&["--since", "1s"]), calls(&["--since", "all"]),
               "the window is not measured from now");

    // AND A SPAN NOBODY DEFINED IS AN ERROR, not a silent `all`.
    assert_eq!(s.run(&["gain", "--since", "whenever"]).status.code(), Some(2));

    // NOTHING READING THIS IS A TERMINAL, so nothing here may be
    // painted: an escape sequence lands in the middle of the token it
    // was meant to highlight, and the usual reader of this program is an
    // agent reading a pipe.
    for args in [&["gain"][..], &["gain", "--json"][..], &["gain", "--thresholds"][..]] {
        let out = text(&s.run(args));
        assert!(!out.contains('\u{1b}'), "{args:?} painted a pipe:\n{out}");
    }
    // AND EVEN WHEN ASKED FOR, JSON STAYS CLEAN — it is not a rendering.
    let out = text(&s.run_with(&[("JBX_COLOR", "always")], &["gain", "--json"]));
    assert!(!out.contains('\u{1b}'), "`--json` was painted:\n{out}");
    serde_json::from_str::<serde_json::Value>(out.trim()).expect("still valid JSON");
    // …while the table takes the paint it was told to take.
    let painted = text(&s.run_with(&[("JBX_COLOR", "always")], &["gain"]));
    assert!(painted.contains('\u{1b}'), "`color: always` painted nothing:\n{painted}");
}

#[test]
#[cfg(unix)]
fn colour_costs_a_column_nothing() {
    // A COLOURED CELL IS WIDER THAN IT LOOKS, and not by a constant:
    // `\x1b[2m` is four characters where `\x1b[32m` is five. A table that
    // pads on the raw string puts a dim row one column further right
    // than a green one.
    //
    // That stayed invisible for as long as the coloured column was the
    // LAST one — a stagger needs something after it to push. `impact` is
    // that something, so this guard needs TWO PROJECTS THAT DID
    // DIFFERENTLY: one that detached and saved, painted green, and one
    // that never did, painted dim.
    let s = Scratch::new("colour-width");
    let root = s.jobs().parent().unwrap().to_path_buf();
    let make = |name: &str| {
        let d = root.join(name);
        std::fs::create_dir_all(d.join(".claude")).unwrap();
        d
    };
    let saver = make("saver");
    let plodder = make("plodder");

    let out = s.run_in(&saver, "after: 0.05\n", &["run", "--", "sleep 1"]);
    let id = announced(&text(&out));
    s.run(&["wait", &id]);
    s.run_in(&plodder, "after: 99\n", &["run", "--", "echo nothing gained"]);

    let bars = |env: &[(&str, &str)]| -> Vec<usize> {
        let out = text(&s.run_with(env, &["gain"]));
        out.lines()
            .filter_map(|line| {
                // The escapes come out; what is left is what a reader sees.
                let mut bare = String::new();
                let mut escaping = false;
                for c in line.chars() {
                    if escaping {
                        escaping = c != 'm';
                    } else if c == '\u{1b}' {
                        escaping = true;
                    } else {
                        bare.push(c);
                    }
                }
                // A TABLE ROW, NOT THE HEADLINE'S OWN METER: rows begin
                // with a project name, the headline's lines are indented.
                let at = bare.find(['\u{2588}', '\u{2591}'])?;
                (!bare.starts_with(' ')).then(|| bare[..at].chars().count())
            })
            .collect()
    };
    let painted = bars(&[("JBX_COLOR", "always")]);
    let plain = bars(&[("JBX_COLOR", "never")]);
    assert!(painted.len() >= 2, "needed two rows to compare, got {painted:?}");
    // WITHIN ONE RENDERING FIRST — this is the half that failed before
    // the table learned to measure visible width rather than bytes.
    assert!(
        painted.iter().all(|c| *c == painted[0]),
        "colour moved the bars apart: {painted:?}"
    );
    // AND COLOUR COSTS NOTHING AT ALL against the unpainted rendering.
    assert_eq!(plain, painted, "painted and plain disagree: {plain:?} vs {painted:?}");
}

#[test]
#[cfg(unix)]
fn only_a_job_that_let_go_counts_as_reached_for() {
    // THE POINT OF THE COLUMN IS THAT THE ACT NEEDED A NAME. Reaching
    // for a job that never let go is reaching for something the caller
    // was standing over anyway — it proves nothing about wrapping, and
    // counting it would turn a narrow true number into a flattering one.
    //
    // THE NEGATIVE CASE IS NOT "a job that did not detach": a record is
    // deleted when its line finishes in time, so there is nothing left
    // to reach for. It is the record that never SAID — written by a
    // version before the field existed. That one answers None, and None
    // must not be read as "yes"; asserting detachment nobody observed is
    // how a narrow number starts flattering.
    let s = Scratch::new("grips");
    let out = s.run(&["run", "--after", "0.1", "--", "sleep 30"]);
    let detached = announced(&text(&out));

    // An older record, made by copying a real one and taking the field
    // back out — closer to the truth than a hand-written stub, which
    // would only prove that our own guess parses.
    let older = "j0000001";
    let mut record: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(s.jobs().join(format!("{detached}.json"))).unwrap(),
    )
    .unwrap();
    record["id"] = serde_json::json!(older);
    record.as_object_mut().unwrap().remove("detached");
    std::fs::write(
        s.jobs().join(format!("{older}.json")),
        serde_json::to_string(&record).unwrap(),
    )
    .unwrap();
    s.run(&["tail", older]);

    // A JOB THAT LET GO, REACHED FOR TWICE BY DIFFERENT VERBS.
    s.run(&["tail", &detached]);
    s.run(&["kill", &detached]);
    s.stop_everything();

    let seen = readings(&s);
    let touches: Vec<&serde_json::Value> =
        seen.iter().filter(|r| r["kind"] == "touch").collect();
    let verbs: std::collections::BTreeSet<&str> =
        touches.iter().filter_map(|t| t["verb"].as_str()).collect();
    assert_eq!(verbs, ["killed", "read"].into_iter().collect(),
               "the verbs recorded were {verbs:?}");
    assert!(touches.iter().all(|t| t["id"] == detached.as_str()),
            "a record that never said it detached was counted: {touches:?}");

    // AND THE HEADLINE COUNTS THE JOB ONCE, not the two gestures. The
    // question is how often a name was worth having, not how often
    // somebody typed.
    let doc: serde_json::Value =
        serde_json::from_str(&text(&s.run(&["gain", "--json"]))).expect("gain is JSON");
    assert_eq!(doc["grips"]["jobs"], 1, "one job, two gestures: {}", doc["grips"]);
    assert_eq!(doc["grips"]["killed"], 1);
    assert_eq!(doc["grips"]["read"], 1);

    // AND IT IS SHOWN. Writing the reading is half the job; a number
    // nobody sees answers nothing.
    //
    // A KILLED JOB LEAVES NO READING OF ITS OWN — the run is recorded
    // when it finishes, and this one was stopped before it could. So a
    // store whose only command was killed renders "nothing measured yet"
    // and the grip goes unseen. Found by this test, not reasoned about:
    // the line below exists so there is something to render beside it.
    // It means `reached for` can name a job that `detached` never
    // counted, which is worth knowing before reading the two together.
    s.run(&["run", "--", "echo done"]);
    let shown = text(&s.run(&["gain"]));
    assert!(shown.contains("reached for"), "the headline says nothing: {shown}");
}

#[test]
#[cfg(unix)]
fn waiting_on_a_job_clears_that_ending_and_no_other() {
    // AN INSTALL WITH ONLY THE WRAPPING HOOK HAS NOTHING THAT EMPTIES
    // THIS BOX. The announcing hooks used to do it every turn; without
    // them an ending would sit unread until the session died, and then
    // be listed as stranded for ever — one box per session, an alarm
    // that always rings and is therefore never read.
    //
    // `jbx wait` IS the delivery, so it clears what it delivered. The
    // hard half is that it must clear THAT ending and no other: emptying
    // the box would discard the endings nobody has collected, which are
    // precisely the ones worth keeping.
    let s = Scratch::new("forget");
    let mut ids = Vec::new();
    for _ in 0..2 {
        let out = s.run(&["run", "--after", "0.1", "--", "sleep 0.6"]);
        ids.push(announced(&text(&out)));
    }
    assert_ne!(ids[0], ids[1], "the two jobs got one id");

    // Both endings land; only one is waited on.
    until("both endings are in the box", || signals_of(&s).len() == 2);
    s.run(&["wait", &ids[0]]);

    let left = signals_of(&s);
    assert_eq!(left.len(), 1, "the box holds {left:?}");
    assert_eq!(left[0]["id"], ids[1].as_str(),
               "the wrong ending survived: {left:?}");

    // AND THE PERSON'S MAIL IS UNTOUCHED. An agent waiting on a job is
    // no reason to throw away what a human has not read.
    let theirs = std::fs::read_to_string(s.jobs().join("signals/user.jsonl")).unwrap_or_default();
    assert_eq!(theirs.lines().filter(|l| !l.trim().is_empty()).count(), 2,
               "waiting ate the person's mail: {theirs}");

    // Waiting on the second clears the box entirely, and waiting twice
    // on the same job is not an error.
    s.run(&["wait", &ids[1]]);
    s.run(&["wait", &ids[1]]);
    assert_eq!(signals_of(&s).len(), 0, "the box still holds {:?}", signals_of(&s));
}

#[test]
fn init_declares_in_the_named_client_and_nowhere_else() {
    // THE WHOLE POINT IS THE ADDRESS. Writing Gemini's hook into
    // Claude's settings would leave both clients wrong and neither
    // complaining: Claude would call jbx on an event it never sends, and
    // Gemini would call nothing at all.
    let s = Scratch::new("init-cli");
    let home = s.jobs().parent().unwrap().join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::create_dir_all(home.join(".gemini")).unwrap();
    let at = |dir: &str| home.join(dir).join("settings.json");
    let events = |dir: &str| -> std::collections::BTreeSet<String> {
        std::fs::read_to_string(at(dir))
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v["hooks"].as_object().map(|o| o.keys().cloned().collect()))
            .unwrap_or_default()
    };
    // BOTH NAMES FOR THE SAME IDEA. jbx reads `USERPROFILE` on Windows
    // and `HOME` everywhere else, so a test that redirects only one of
    // them redirects nothing there — and worse, writes into the real
    // profile while looking for the answer in the fake one. Found by CI,
    // which is the only machine here that runs Windows.
    let env = [
        ("HOME", home.to_str().unwrap()),
        ("USERPROFILE", home.to_str().unwrap()),
    ];

    s.run_with(&env, &["init", "--global-only", "--cli", "gemini"]);
    assert_eq!(events(".gemini"), ["BeforeTool".to_string()].into_iter().collect(),
               "gemini got {:?}", events(".gemini"));

    // AND THE DECLARED COMMAND NAMES THE CLIENT. This guard once checked
    // only WHICH events were declared, never WHAT they call — so it
    // passed while `jbx init --cli gemini` wrote a bare `jbx hook`, which
    // answers as Claude, finds `BeforeTool` unfamiliar and returns 0.
    // Installed, matching, silent, useless: the exact failure the table
    // exists to prevent, missed by the test written to prevent it.
    let declared = std::fs::read_to_string(at(".gemini")).unwrap();
    assert!(declared.contains("hook gemini"),
            "the declaration does not name the dialect: {declared}");
    assert!(events(".claude").is_empty(), "claude was touched: {:?}", events(".claude"));

    // AND `--announce` SPEAKS THAT CLIENT'S NAMES. Declaring Claude's
    // `Stop` in Gemini's settings would be a hook that never fires and
    // looks installed.
    s.run_with(&env, &["init", "--global-only", "--cli", "gemini", "--announce"]);
    assert_eq!(
        events(".gemini"),
        ["BeforeTool", "AfterAgent", "BeforeAgent", "SessionStart"]
            .map(String::from).into_iter().collect::<std::collections::BTreeSet<_>>(),
        "gemini got {:?}", events(".gemini")
    );

    // `--core` takes them back, in the same client.
    s.run_with(&env, &["init", "--global-only", "--cli", "gemini", "--core"]);
    assert_eq!(events(".gemini"), ["BeforeTool".to_string()].into_iter().collect());

    // AND THE OTHER CLIENT IS STILL UNTOUCHED after all of that.
    assert!(events(".claude").is_empty(), "claude was touched: {:?}", events(".claude"));

    // A CLIENT WE CAN ANSWER BUT NOT DECLARE FOR SAYS SO. Cursor keeps
    // its hooks in `.cursor/hooks.json`, Copilot in `~/.copilot/hooks/`,
    // Droid in `~/.factory/hooks.json` with the events at the root —
    // three shapes that are not this one. Writing Claude's into any of
    // them would install a hook that never fires.
    let cannot = s.run_with(&env, &["init", "--global-only", "--cli", "cursor"]);
    assert_eq!(cannot.status.code(), Some(1), "init claimed to declare for cursor");
    let said = String::from_utf8_lossy(&cannot.stderr).to_string();
    assert!(said.contains("hook cursor"),
            "the refusal does not say what DOES work: {said}");
    assert!(events(".claude").is_empty() && !at(".cursor").exists(),
            "the refusal still wrote something");

    // A NAME NOBODY WIRED IS REFUSED, and says what is known — the
    // listing exists so that answer is findable before the mistake.
    let bad = s.run_with(&env, &["init", "--global-only", "--cli", "windsurf"]);
    assert_eq!(bad.status.code(), Some(2), "an unknown client was accepted");
    let told = text(&s.run(&["hook", "--list"]));
    assert!(told.contains("claude") && told.contains("gemini"),
            "the listing does not name what works: {told}");
    assert!(told.contains(".gemini/settings.json"),
            "the listing does not say where it declares: {told}");
}

#[test]
#[cfg(unix)]
fn the_old_house_is_carried_over_and_nothing_is_left_behind() {
    // WHAT WAS NEVER CACHE WAS LIVING IN ONE. The readings and the
    // record of the hooks `init` displaced sat under `~/.cache`, whose
    // contract is that it may be emptied at any hour — so weeks of
    // measurement and the ability to uninstall depended on nobody
    // tidying up. Moving house is the fix; losing the furniture in the
    // move would be worse than the problem.
    let s = Scratch::new("moving");
    let home = s.jobs().parent().unwrap().join("home");
    let old_jobs = home.join(".cache/jbx/jobs");
    std::fs::create_dir_all(old_jobs.join("sessions")).unwrap();
    std::fs::create_dir_all(home.join(".config/jobbox")).unwrap();

    std::fs::write(old_jobs.join("stats.jsonl"), "{\"kind\":\"run\",\"secs\":1}\n").unwrap();
    std::fs::write(old_jobs.join("displaced-hooks.json"), "{\"displaced\":[]}").unwrap();
    std::fs::write(old_jobs.join("j1234567.log"), "a log").unwrap();
    std::fs::write(home.join(".config/jobbox/config.yaml"), "after: 42\n").unwrap();

    // HOME ONLY, AND NOTHING ELSE. The usual helper pins `JBX_DIR` and
    // `JBX_CONFIG`, and the move deliberately does not happen for
    // somebody who has said where things go — pinning them here would
    // test the one case this cannot apply to.
    let ask = |args: &[&str]| -> String {
        let out = Command::new(JBX)
            .env_remove("JBX_WRAPPED")
            .env_remove("JBX_DIR")
            .env_remove("JBX_CONFIG")
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("the binary runs");
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    };
    // Any command at all: the move happens the first time jbx is asked
    // where it keeps things, not from a verb somebody has to know about.
    let shown = ask(&["config"]);

    let now = home.join(".jobbox");
    assert!(now.join("readings.jsonl").exists(), "the readings were left behind");
    assert!(now.join("displaced-hooks.json").exists(), "the undo record was left behind");
    assert!(now.join("cache/j1234567.log").exists(), "the logs were left behind");
    assert!(now.join("config.yaml").exists(), "the settings were left behind");

    // AND THE SETTINGS ARE READ, not merely moved — the point of the
    // move is that jbx finds them afterwards.
    assert!(shown.contains("42"), "the moved settings were not read:\n{shown}");

    // MOVED, NOT COPIED. Two truths and no way to tell which is current
    // is the failure a copy leaves behind.
    assert!(!old_jobs.join("stats.jsonl").exists(), "the readings were copied, not moved");
    assert!(!home.join(".config/jobbox/config.yaml").exists(), "the settings were copied");

    // AND IT DOES NOT RUN TWICE. A second call must find nothing to do
    // rather than announce a move it did not make.
    let again = ask(&["config"]);
    assert!(!again.contains("moved"), "it moved things a second time:\n{again}");
}

#[test]
#[cfg(unix)]
fn a_quiet_session_is_not_the_same_as_a_dead_one() {
    // THE HALF THAT BREAKS IN SILENCE. Clearing a mailbox whose reader
    // is gone is tidying; clearing one whose reader is merely quiet is
    // taking somebody's mail before they read it — and they never learn
    // it existed. `stranded()` calls every box that is not ours
    // stranded, which is fine for showing and wrong for taking, so the
    // sweep goes by AGE and this is what says so.
    let s = Scratch::new("sweeping");
    let dir = s.jobs().join("signals");
    let ending = |id: &str| format!("{{\"id\":\"{id}\",\"code\":0,\"intent\":\"x\",\"log\":\"/l\",\"client\":\"c\"}}\n");

    // One box left long ago, one from a session that stepped out for tea.
    for who in ["long-gone", "just-quiet"] {
        std::fs::create_dir_all(dir.join(who)).unwrap();
        std::fs::write(dir.join(who).join("agent.jsonl"), ending(&format!("j{who:.7}"))).unwrap();
    }
    // The person already holds the abandoned one — every ending is
    // deposited to both boxes at once, which is why sweeping is dropping
    // a copy rather than throwing mail away.
    std::fs::write(dir.join("user.jsonl"), ending("jlong-go")).unwrap();

    // Age is the whole signal, so it is set rather than waited for.
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(9 * 3600);
    let f = std::fs::File::options().write(true).open(dir.join("long-gone/agent.jsonl")).unwrap();
    f.set_modified(old).unwrap();

    s.run(&["health"]);

    assert!(!dir.join("long-gone").exists(), "the abandoned box was left to grow");
    assert!(dir.join("just-quiet").exists(), "a quiet session's mail was taken");
    let theirs = std::fs::read_to_string(dir.join("just-quiet/agent.jsonl")).unwrap();
    assert!(theirs.contains("jjust-qu"), "the quiet box was emptied: {theirs}");

    // AND NOTHING WAS DUPLICATED. The abandoned ending was already the
    // person's; carrying it over again would turn a tidy-up into noise.
    let mail = std::fs::read_to_string(dir.join("user.jsonl")).unwrap();
    assert_eq!(mail.matches("jlong-go").count(), 1, "the ending was copied twice:\n{mail}");
}

#[test]
fn every_declared_dialect_really_answers() {
    // WHAT THIS CAN PROVE, AND WHAT IT CANNOT.
    //
    // It cannot check jbx against a real Gemini: that needs Gemini. What
    // it can do is hold the MEASURED contract still — the shapes below
    // were found by sending payloads at each client's own hook processor
    // — so that editing the table without re-measuring fails here rather
    // than in somebody's session.
    //
    // AND THE EXPECTATIONS ARE WRITTEN OUT, NOT READ FROM THE TABLE. A
    // first version of this test took the tool name and the output path
    // from `jbx describe` and then checked the answer at that same path,
    // which is a sentence agreeing with itself: pointing the dialect at
    // the wrong tool passed cleanly. Measured, by breaking it on purpose.
    //
    // THE TOOL NAMES ARE THE HALF MOST WORTH PINNING. Three of these
    // were probed through rtk first, and rtk watches for `Bash` on all
    // three — where Cursor says `Shell`, Droid says `Execute` and
    // Copilot says `bash`. A wrong name is a hook that never fires and
    // looks perfectly healthy, so the names come from each client's own
    // reference and are written out here to stay there.
    const MEASURED: [(&str, &str, &str, &[&str], bool); 5] = [
        ("claude",  "Bash",              "PreToolUse", &["hookSpecificOutput", "updatedInput"], true),
        ("gemini",  "run_shell_command", "BeforeTool", &["hookSpecificOutput", "tool_input"],   false),
        ("droid",   "Execute",           "PreToolUse", &["hookSpecificOutput", "updatedInput"], true),
        ("cursor",  "Shell",             "preToolUse", &["updated_input"],                      true),
        ("copilot", "bash",              "preToolUse", &["modifiedArgs"],                       true),
    ];

    let s = Scratch::new("dialects");
    let doc: serde_json::Value =
        serde_json::from_str(&text(&s.run(&["describe"]))).expect("describe is JSON");
    let published = doc["x-jbx-dialects"].as_array().expect("dialects are published");
    assert_eq!(published.len(), MEASURED.len(),
               "the table has {} dialects and this test knows {}: re-measure, then update both",
               published.len(), MEASURED.len());

    for (name, tool, event, at, replaces) in MEASURED {
        let d = published.iter().find(|d| d["name"] == name)
            .unwrap_or_else(|| panic!("{name} is not published: {published:?}"));
        assert_eq!(d["tool"], tool, "{name}'s tool name moved");
        assert_eq!(d["before_tool"], event, "{name}'s event name moved");
        assert_eq!(d["at"], serde_json::json!(at), "{name}'s output path moved");
        assert_eq!(d["replaces_input"], replaces, "{name}'s merge behaviour moved");

        // AND IT REALLY ANSWERS. A hook handed a payload it does not
        // recognise says nothing and exits 0 — the same as a healthy
        // hook seeing a tool it does not watch — so silence cannot be
        // read as "no rewrite needed", and a dialect shipped unwired
        // looks exactly like one that works.
        // COPILOT'S OWN FORMAT NAMES ITS FIELDS DIFFERENTLY — `toolName`
        // and `toolArgs` where the others say `tool_name`/`tool_input`.
        // Reading the wrong key finds nothing, and finding nothing is
        // indistinguishable from a tool we were not watching.
        let (k_tool, k_input) = if name == "copilot" {
            ("toolName", "toolArgs")
        } else {
            ("tool_name", "tool_input")
        };
        let payload = format!(
            r#"{{"hook_event_name":"{event}","{k_tool}":"{tool}","{k_input}":{{"command":"echo hi","timeout":600000}}}}"#
        );
        let out = text(&s.event_as(name, &payload));
        let answer: serde_json::Value = serde_json::from_str(out.trim())
            .unwrap_or_else(|e| panic!("{name} answered {out:?}: {e}"));
        let mut here = &answer;
        for key in at {
            here = &here[*key];
        }
        let line = here["command"].as_str()
            .unwrap_or_else(|| panic!("{name}: nothing at {at:?} in {answer}"));
        assert!(line.contains("run") && line.contains("echo hi"),
                "{name} did not wrap the line: {line}");

        // A CLIENT THAT REPLACES ITS INPUT GETS EVERY FIELD BACK.
        // `timeout` is the caller saying how long they were prepared to
        // wait; dropping it answers a different question than the one
        // asked.
        if replaces {
            assert_eq!(here["timeout"], 600000,
                       "{name} replaces its input but lost `timeout`: {answer}");
        }

        // NOTHING IS GRANTED ON THE CALLER'S BEHALF, in any dialect.
        // This hook wraps every command, so one decision here decides
        // for all of them.
        for key in ["decision", "permissionDecision"] {
            assert!(answer.get(key).is_none(), "{name} took a decision: {answer}");
            assert!(answer[at[0]].get(key).is_none(), "{name} took a decision: {answer}");
        }
    }

    // AND A NAME NOBODY WIRED IS REFUSED, not quietly treated as Claude.
    let unknown = s.event_as("windsurf", "{}");
    assert_eq!(unknown.status.code(), Some(2), "an unknown dialect was accepted");
    let said = String::from_utf8_lossy(&unknown.stderr).to_string();
    assert!(said.contains("claude") && said.contains("gemini"),
            "the refusal does not say what IS known: {said}");
}

#[test]
#[cfg(unix)]
fn a_watch_reports_every_ending_and_then_stops() {
    let s = Scratch::new("watch");
    // NOTHING RUNNING IS NOT A HANG. A watch armed for ever after its
    // event has fired is the failure the harness warns about, so an
    // empty store ends it at once.
    let idle = s.run(&["watch"]);
    assert_eq!(idle.status.code(), Some(0), "an idle watch did not end");
    assert!(text(&idle).trim().is_empty(), "an idle watch invented events: {}", text(&idle));

    // ONE THAT SUCCEEDS AND ONE THAT DOES NOT. A watch that only speaks
    // on success is silent through a crash, and silence looks exactly
    // like "still running" — so the failing one has to be in here.
    s.run(&["run", "--after", "0", "--intent", "the one that works", "--", "sleep 1"]);
    s.run(&["run", "--after", "0", "--intent", "the one that fails", "--", "sleep 1; exit 5"]);
    let seen = text(&s.run(&["watch"]));
    assert!(seen.contains("finished 0"), "the ending that worked went unsaid:\n{seen}");
    assert!(seen.contains("finished 5"), "the FAILURE went unsaid:\n{seen}");
    assert!(seen.contains("the one that fails"), "the line was not named:\n{seen}");

    // AND THE STREAM IS ONE OBJECT PER LINE, not an array: an array is
    // valid only once closed, and a stream closes when it is over.
    s.run(&["run", "--after", "0", "--", "sleep 1"]);
    let streamed = text(&s.run(&["watch", "--json"]));
    let lines: Vec<&str> = streamed.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(!lines.is_empty(), "the json watch said nothing");
    for line in lines {
        serde_json::from_str::<serde_json::Value>(line)
            .unwrap_or_else(|e| panic!("a stream line was not an object ({e}): {line}"));
    }
}

#[test]
fn a_command_the_harness_already_backgrounded_is_never_detached() {
    let s = Scratch::new("already-bg");
    let asked = |bg: bool| {
        let extra = if bg { r#","run_in_background":true"# } else { "" };
        let answer: serde_json::Value = serde_json::from_str(&s.event(
            "cc-bg",
            &format!(
                r#"{{"hook_event_name":"PreToolUse","tool_name":"Bash",
                    "tool_input":{{"command":"make lint"{extra}}}}}"#
            ),
        ))
        .expect("valid JSON");
        answer["hookSpecificOutput"]["updatedInput"]["command"]
            .as_str()
            .unwrap_or("")
            .to_string()
    };

    // THE HARNESS NOTIFIES WHEN THE COMMAND IT BACKGROUNDED EXITS. A
    // wrapper that detaches underneath exits at the threshold, so the
    // notification fires at thirty seconds and says the work is done
    // when it has barely started — the exact lie this program exists to
    // prevent, introduced by the program.
    let backgrounded = asked(true);
    assert!(backgrounded.contains("--after inf"),
            "a command already in the background was left detachable: {backgrounded}");

    // AND NOTHING CHANGES FOR AN ORDINARY ONE, which is the half that
    // makes the assertion above mean anything.
    let ordinary = asked(false);
    assert!(!ordinary.contains("--after"),
            "an ordinary command was pinned to the foreground: {ordinary}");
    assert!(ordinary.contains("run "), "an ordinary command stopped being wrapped: {ordinary}");
}

#[test]
fn the_plugin_declares_what_init_declares_and_says_the_same_version() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap_or_else(|e| panic!("{p}: {e}"));

    // ONE VERSION, THREE FILES. A plugin manifest that lags the crate
    // ships a version number that is a claim about code it does not
    // contain — and nobody would notice, because nothing else reads it.
    let manifest: serde_json::Value =
        serde_json::from_str(&read("plugin/.claude-plugin/plugin.json")).expect("valid JSON");
    let crate_version = env!("CARGO_PKG_VERSION");
    assert_eq!(manifest["version"], crate_version,
               "the plugin manifest is at {} while the crate is at {crate_version}",
               manifest["version"]);

    // AND THE SAME EVENTS AS AN ANNOUNCING `jbx init`. Two ways in that
    // declare different hooks are two behaviours wearing one name;
    // whichever a reader installs, they get the one they did not read
    // about. The plugin cannot take a flag, so it is the ANNOUNCING
    // install — and this says which, rather than leaving it to be
    // discovered.
    //
    // IT USED TO GREP THE SOURCE for event names, which made it blind to
    // the only change that could ever break it: `init` still MENTIONS
    // all four while declaring one by default. So it asks the binary
    // now, and compares what is actually written.
    let hooks: serde_json::Value =
        serde_json::from_str(&read("plugin/hooks/hooks.json")).expect("valid JSON");
    let declared: std::collections::BTreeSet<String> =
        hooks["hooks"].as_object().expect("an object").keys().cloned().collect();

    let events = |args: &[&str]| -> std::collections::BTreeSet<String> {
        let s = Scratch::new(&format!("plugin-{}", args.join("-")));
        let home = s.jobs().parent().unwrap().join("home");
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        let mut all = vec!["init", "--global-only"];
        all.extend_from_slice(args);
        // `USERPROFILE` TOO — see the note in the sibling test: jbx
        // reads that one on Windows, and redirecting only `HOME` sends
        // the write to the real profile.
        s.run_with(
            &[
                ("HOME", home.to_str().unwrap()),
                ("USERPROFILE", home.to_str().unwrap()),
            ],
            &all,
        );
        let settings: serde_json::Value = std::fs::read_to_string(home.join(".claude/settings.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(serde_json::json!({}));
        settings["hooks"]
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    };
    let announcing = events(&["--announce"]);
    assert_eq!(declared, announcing,
               "the plugin declares {declared:?} where `jbx init --announce` writes {announcing:?}");

    // AND THE DEFAULT IS THE SMALL ONE. Named here because it is the
    // promise the flag exists to keep: plain `init` touches one hook.
    let plain = events(&[]);
    assert_eq!(plain, ["PreToolUse".to_string()].into_iter().collect::<std::collections::BTreeSet<_>>(),
               "`jbx init` with no flag wrote {plain:?}");

    // AND EVERY HOOK GOES THROUGH THE PLUGIN'S OWN BINARY, quoted the
    // way the docs require — an unquoted `${CLAUDE_PLUGIN_ROOT}` breaks
    // on the first install path with a space in it, which is every
    // Windows one.
    for (event, entries) in hooks["hooks"].as_object().unwrap() {
        for entry in entries.as_array().unwrap() {
            for h in entry["hooks"].as_array().unwrap() {
                let cmd = h["command"].as_str().unwrap_or("");
                assert!(cmd.starts_with("\"${CLAUDE_PLUGIN_ROOT}\"/bin/jbx "),
                        "{event} does not call the plugin's own binary, quoted: {cmd}");
            }
        }
    }

    // THE MONITOR IS THE ONE VERB WRITTEN FOR IT. `jbx watch` ends by
    // itself when nothing is running, which is what a monitor needs and
    // what `tail -f` can never do.
    // THE FAÇADE IS FOR A PERSON, NOT FOR THE MODEL. A skill the model
    // may invoke is a document telling it what to do — and a document
    // asking it to judge which commands are long is the thing this
    // project refused and built a program instead of. The discipline
    // reaches the model through the hook, which is the tool speaking.
    // EVERY ONE OF THEM, and `slots` and `after` for a second reason:
    // a model that may set its own threshold can set it to five minutes
    // and stop detaching anything, which is the tool switching itself
    // off to avoid the discipline.
    for name in ["jbx", "slots", "after"] {
        let skill = read(&format!("plugin/skills/{name}/SKILL.md"));
        assert!(skill.contains("disable-model-invocation: true"),
                "the `{name}` skill is model-invocable:\n{skill}");
        // AND THEY DEFER RATHER THAN REPEAT. A second copy of anything
        // is a second copy to keep in step, and the copy is what drifts.
        assert!(skill.contains(&format!("jbx {}", if name == "jbx" { "help" } else { name })),
                "the `{name}` skill does not defer to the binary:\n{skill}");
    }

    let monitors: serde_json::Value =
        serde_json::from_str(&read("plugin/monitors/monitors.json")).expect("valid JSON");
    let command = monitors[0]["command"].as_str().unwrap_or("");
    assert!(command.contains("/bin/jbx watch"), "the monitor does not run `jbx watch`: {command}");
    assert!(monitors[0]["name"].is_string() && monitors[0]["description"].is_string(),
            "a monitor wants a name and a description");
}

#[test]
fn every_verb_the_dispatcher_answers_is_a_declared_one() {
    // A VERB THAT WORKS AND IS NOT DECLARED IS INVISIBLE TO EVERY GUARD.
    //
    // `jbx after` shipped that way for an hour: the dispatcher answered
    // it, the flag parser looked it up in the document, found nothing,
    // and therefore refused EVERY flag — `--json` included — while the
    // README guard and the `--json` invariant both stayed quiet, because
    // both start from the document and the document had never heard of
    // it. Guards that read one list cannot see what is missing from it.
    //
    // So this reads the other side: the match arms themselves.
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/bin/jbx.rs"))
        .expect("the dispatcher");
    let dispatch = &source[source.find("fn dispatch(").expect("dispatch")..];
    let dispatch = &dispatch[..dispatch.find("\nfn ").unwrap_or(dispatch.len())];

    let doc: serde_json::Value = serde_json::from_str(
        text(&Scratch::new("declared").run(&["describe"])).trim(),
    )
    .expect("valid JSON");
    let declared: std::collections::BTreeSet<String> = doc["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();

    let mut undeclared = Vec::new();
    for line in dispatch.lines() {
        let line = line.trim_start();
        // `"verb" => …` and `"a" | "b" => …`, which is how the help and
        // version aliases are written.
        let Some(rest) = line.strip_prefix('"') else { continue };
        let Some((verb, _)) = rest.split_once('"') else { continue };
        // NOT EVERY ARM IS A VERB. `supervise` is one half of this
        // binary talking to the other and is deliberately undeclared —
        // a verb a person can be tempted to type is a verb that will be
        // typed. The flag aliases are not verbs at all.
        if verb.is_empty() || verb.starts_with('-') || verb == "supervise" || verb == "help" {
            continue;
        }
        if !declared.contains(verb) {
            undeclared.push(verb.to_string());
        }
    }
    assert!(undeclared.is_empty(),
            "the dispatcher answers verbs the document never declares: {undeclared:?}");
}

#[test]
fn describe_covers_every_verb_and_invents_none() {
    let s = Scratch::new("describe");
    let doc: serde_json::Value =
        serde_json::from_str(text(&s.run(&["describe"])).trim()).expect("valid JSON");
    let help = text(&s.run(&["--help"]));

    let named: Vec<String> = doc["commands"].as_array().unwrap().iter()
        .map(|c| c["name"].as_str().unwrap().to_string()).collect();

    // BOTH DIRECTIONS, which is the lesson the README taught: a guard
    // that only checks for what is false never notices what is missing.
    for verb in &named {
        assert!(help.contains(&format!("jbx {verb}")),
                "described a verb the binary does not have: {verb}");
    }
    for line in help.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("jbx ") {
            let verb = rest.split_whitespace().next().unwrap_or("");
            if verb.chars().all(|c| c.is_ascii_lowercase()) && !verb.is_empty() {
                assert!(named.contains(&verb.to_string()),
                        "the binary has a verb the document never mentions: {verb}");
            }
        }
    }

    // AND EVERY ONE SAYS WHAT IT DOES TO THE WORLD. That field is the
    // whole reason the document exists — a CLI schema carries the shape
    // of `kill` and never that it tears down a process tree.
    for c in doc["commands"].as_array().unwrap() {
        let effect = c["x-jbx-effect"].as_str().unwrap_or("");
        assert!(!effect.is_empty(), "{} has no effect", c["name"]);
    }
    // TAGS ARE WHAT A GUARD COMPARES; the sentence is for a person.
    let tags_of = |name: &str| -> Vec<String> {
        doc["commands"].as_array().unwrap().iter()
            .find(|c| c["name"] == name)
            .and_then(|c| c["x-jbx-tags"].as_array())
            .map(|t| t.iter().map(|v| v.as_str().unwrap().to_string()).collect())
            .unwrap_or_default()
    };
    assert_eq!(tags_of("list"), vec!["read"]);
    assert_eq!(tags_of("kill"), vec!["destroy"]);
    // AND `signals` IS NOT A READ. Looking at it destroys it, and a
    // guard that took it for a look would let an ending be lost.
    assert_eq!(tags_of("signals"), vec!["consume"]);

    // EVERY TAG USED IS DECLARED IN THE DOCUMENT. A typo would be a tag
    // no reader can match, silently — which is the failure this whole
    // document exists to remove, reappearing one level down.
    let vocabulary = doc["x-jbx-tag-meanings"].as_object().expect("a vocabulary");
    for c in doc["commands"].as_array().unwrap() {
        for tag in c["x-jbx-tags"].as_array().unwrap() {
            let tag = tag.as_str().unwrap();
            assert!(vocabulary.contains_key(tag),
                    "{} carries `{tag}`, which the document never defines", c["name"]);
        }
    }
}

#[test]
fn a_listing_can_show_the_whole_line_and_speak_json() {
    let s = Scratch::new("shapes");
    let line = "cd /tmp && echo a very long line indeed that a column would cut; sleep 30";
    s.run(&["run", "--after", "1", "--", line]);
    // THE JOBS OUTLIVE THE ASSERTIONS ON PURPOSE. What is measured here
    // is how a listing RENDERS what is running, so both jobs have to
    // still BE running when the last row is read. The four `jbx` calls
    // in between cost more wall time on the Windows runner than the four
    // seconds this line used to allow itself, and the listing then had
    // one row where it wanted two. Thirty is not a guess about speed: it
    // is longer than every path through this test.
    until("the long line is listed as running", || {
        text(&s.run(&["ps", "--full"])).contains("would cut")
    });

    // BOTH COLUMNS WHEN THERE ARE TWO THINGS TO SAY. Nobody named this
    // one, so its intent would be the first four words of the line
    // printed beside the line — a column that repeats its neighbour. It
    // appears when a caller actually said something, and not before.
    let short = text(&s.run(&["ps", "--width", "80"]));
    assert!(!short.contains("intent"), "a column of nothing was drawn: {short}");
    assert!(short.contains("echo a very long line"), "the line column is gone: {short}");
    s.run(&["run", "--after", "1", "--intent", "measure the index", "--", "sleep 3"]);
    let both = text(&s.run(&["ps"]));
    assert!(both.contains("intent"), "a named job drew no intent column: {both}");
    assert!(both.contains("measure the index"), "the name was dropped: {both}");
    assert!(!short.contains("/tmp &&"), "the compact line still carries the cd: {short}");
    assert!(!short.contains("would cut"), "the default stopped truncating: {short}");
    let full = text(&s.run(&["ps", "--full"]));
    assert!(full.contains("would cut"), "--full still cut the line: {full}");

    let rows: serde_json::Value =
        serde_json::from_str(text(&s.run(&["ps", "--json"])).trim()).expect("valid JSON");
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    // JSON DROPS NOTHING — neither the line the table cut, nor the name
    // the table left out because it repeated that line.
    let cut_one = rows.iter().find(|r| r["command"].as_str().unwrap().contains("would cut"));
    let cut_one = cut_one.expect("the long line is in the JSON");
    // AND THE NAME IGNORES A LEADING `cd`. The harness writes one in
    // front of every command, so four words of path named nothing.
    assert!(cut_one["intent"].as_str().unwrap().starts_with("echo"),
            "the name is still four words of path: {}", cut_one["intent"]);
    assert!(rows.iter().any(|r| r["intent"] == "measure the index"),
            "a name somebody gave is missing from the JSON");
    s.stop_everything();
}

#[test]
#[cfg(unix)]
fn a_compact_listing_drops_the_wrappers_a_line_arrives_in() {
    let s = Scratch::new("preamble");
    // THE SHAPE THIS WAS MEASURED ON. Twenty of fifty records in a real
    // store began with all three wrappers: forty characters of identical
    // preamble standing exactly where the difference between two jobs
    // should be. `rtk` is not on the PATH here and does not need to be —
    // what is under test is what the table prints, not what runs.
    // A JOB ONLY OUTLIVES ITS LINE WHEN IT WAS DETACHED — a line that
    // finishes in time leaves nothing behind, on purpose — so each of
    // these outlasts `--after`. The `sleep` is at the END: one written
    // in FRONT is exactly what this trims.
    s.run(&["run", "--after", "1", "--", "cd /tmp && timeout 300 rtk proxy echo real; sleep 2"]);
    // AND WHAT MUST SURVIVE IT. `rtk gain` is a command of rtk's own,
    // `sleep 2` is the whole of the work, and a duration that is not one
    // means `timeout` was never the wrapper it looked like.
    s.run(&["run", "--after", "1", "--", "rtk gain --history; sleep 2"]);
    s.run(&["run", "--after", "1", "--", "sleep 2"]);
    s.run(&["run", "--after", "1", "--", "timeout 300; echo kept; sleep 2"]);

    let listed = text(&s.run(&["list"]));
    assert!(listed.contains("echo real"), "the payload never showed:\n{listed}");
    assert!(!listed.contains("rtk proxy"), "a wrapper survived the trim:\n{listed}");
    assert!(!listed.contains("timeout 300 rtk"), "a wrapper survived the trim:\n{listed}");
    assert!(listed.contains("rtk gain --history"), "rtk's own verb was eaten:\n{listed}");
    assert!(listed.contains("sleep 2"), "the work itself was eaten:\n{listed}");
    assert!(listed.contains("timeout 300; echo kept"),
            "`timeout` was trimmed without a duration to justify it:\n{listed}");

    // AND THE DERIVED NAME TOO — four words of `timeout 300 rtk proxy`
    // name the envelope, which is the fault the leading `cd` had.
    let rows: serde_json::Value =
        serde_json::from_str(text(&s.run(&["list", "--json"])).trim()).expect("valid JSON");
    let named: Vec<&str> =
        rows.as_array().unwrap().iter().map(|r| r["intent"].as_str().unwrap()).collect();
    assert!(named.iter().any(|n| n.starts_with("echo real")),
            "the name is still the envelope: {named:?}");
    // THE LINE ITSELF IS UNTOUCHED. What is dropped is reading room, and
    // `--full` and `--json` are where the whole of it lives.
    assert!(rows.as_array().unwrap().iter().any(|r| r["command"]
        .as_str()
        .unwrap()
        .starts_with("cd /tmp && timeout 300 rtk proxy")),
        "the record lost the wrappers, not just the table");
}

#[test]
fn a_job_is_named_by_whoever_ran_it_when_they_said() {
    let s = Scratch::new("named");
    // THE HARNESS ALREADY ASKS what each command is for, and hands the
    // answer to the hook. Four words off the front of the line name
    // nothing when every line starts the same way.
    let said = text(&s.run(&[
        "run", "--after", "1", "--intent", "replay the DAG simulation", "--", "sleep 30",
    ]));
    let id = announced(&said);
    // SIX LISTINGS READ THE SAME JOB, so it has to outlast all six. At
    // three seconds it did not on the Windows runner: the first `ps`
    // answered "nothing running here" and the name looked dropped when
    // it was only late. The job is stopped at the end rather than
    // waited on, since nothing here needs its ending.
    until("the named job is listed as running", || {
        text(&s.run(&["ps", "--width", "200"])).contains("replay the DAG simulation")
    });
    let listed = text(&s.run(&["ps", "--width", "200"]));
    assert!(listed.contains("replay the DAG simulation"), "the name was dropped:\n{listed}");
    // AND IT IS CUT TO THE COLUMN, not to a number written years ago.
    // NINETY, NOT A HUNDRED: an empty mute column no longer reserves ten
    // blanks, and at a hundred those ten columns let this name fit whole.
    let narrow = text(&s.run(&["ps", "--width", "90"]));
    assert!(!narrow.contains("replay the DAG simulation"), "90 columns drew 200:\n{narrow}");
    assert!(narrow.contains("replay the"), "the name went missing entirely:\n{narrow}");

    // AND NO ROW OUTRUNS THE WIDTH IT WAS GIVEN. One character over is
    // invisible until a full-screen terminal wraps every line of the
    // table — which is how it shipped: the space after the intent cell
    // was printed by the cell and counted by nobody.
    for wide in [80usize, 100, 137, 200] {
        let drawn = text(&s.run(&["ps", "--all", "--width", &wide.to_string()]));
        for row in drawn.lines() {
            assert!(row.chars().count() <= wide,
                    "a row ran {} past a {wide}-column table:\n{row}",
                    row.chars().count() - wide);
        }
    }

    // AND THE HOOK FILLS IT ON ITS OWN, from the description the harness
    // gives it — so it costs nobody anything to type.
    let event = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "make test", "description": "run the unit tests", "timeout": 1},
    }).to_string();
    let mut child = Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .arg("hook")
        .env("JBX_DIR", &s.0)
        .env("JBX_CONFIG", s.0.join("global.yaml"))
        .env("PATH", "/nonexistent")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(event.as_bytes()).unwrap();
    let out = String::from_utf8_lossy(&child.wait_with_output().unwrap().stdout).into_owned();
    let answer: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    let rewritten = answer["hookSpecificOutput"]["updatedInput"]["command"].as_str().unwrap();
    assert!(rewritten.contains("--intent 'run the unit tests'"),
            "the description did not travel: {rewritten}");
    let _ = s.run(&["kill", &id]);
}

#[test]
fn the_detachment_offers_monitor_only_where_monitor_exists() {
    // SAYING "DO NOT WAIT" AND THEN OFFERING THE BARE LINE THAT WAITS is
    // a contradiction, and an agent resolves it the easy way: it pastes
    // what it was given. Reported as agents calling `jbx wait` on every
    // single detachment, which puts them straight back to standing
    // still — having spent a turn to get there.
    //
    // But handing that wait to Monitor IS the right gesture under Claude
    // Code: it ends when the job does, so the ending wakes the session.
    // So the sentence is there, and it names the vehicle.
    let s = Scratch::new("nowait");
    let claude = [("CLAUDE_CODE_SESSION_ID", "abc123")];
    let said = text(&s.run_with(&claude, &["run", "--after", "0", "--", "sleep 2"]));
    assert!(said.contains("DO NOT WAIT FOR IT"), "the instruction is gone:\n{said}");
    assert!(said.contains("Monitor"), "the right gesture is not named:\n{said}");
    assert!(said.contains("Do not run it in front of you"), "the wrong one is not ruled out:\n{said}");

    // AND NOWHERE ELSE. On a client with no Monitor the word means
    // nothing, so the announcement does not say it — `jbx help <id>`
    // lists what can be done instead.
    let plain = [("CLAUDE_CODE_SESSION_ID", "")];
    let elsewhere = text(&s.run_with(&plain, &["run", "--after", "0", "--", "sleep 2"]));
    assert!(!elsewhere.contains("Monitor"), "Monitor was named where it does not exist:\n{elsewhere}");
    assert!(!elsewhere.contains("jbx wait"), "the bare waiting line was offered:\n{elsewhere}");
    assert!(elsewhere.contains("jbx help"), "and nothing points anywhere:\n{elsewhere}");
}

#[test]
fn only_the_agents_own_wait_is_refused() {
    // OFF BY DEFAULT, AND OFF MEANS *FOR THE AGENT*. The mark is written
    // by the hook onto a `jbx wait` the agent typed into its own shell.
    // What Monitor launches never passes through the hook, so it is
    // never marked — and that asymmetry is the whole discrimination.
    let s = Scratch::new("allowwait");

    let refused = s.run(&["wait", "j0000000", "--via-agent"]);
    assert_eq!(refused.status.code(), Some(2), "a refusal must not look like a job's own code");
    // ON STDERR: a refusal is a diagnostic, not the answer that was asked for.
    let said = String::from_utf8_lossy(&refused.stderr).into_owned();
    assert!(said.contains("Monitor"), "it refused without saying what to do:\n{said}");
    // AND IT DOES NOT NAME THE SETTING. Telling an agent which knob
    // forbade this is telling it where to go and switch the guardrail
    // off — and editing a config file is what it does all day.
    assert!(!said.contains("allow_wait"), "the refusal handed over the way around it:\n{said}");

    // UNMARKED IS MONITOR'S PATH, and it must go straight through. An
    // unknown id is an unknown id, not a refusal.
    let allowed = s.run(&["wait", "j0000000"]);
    assert_ne!(allowed.status.code(), Some(2), "Monitor's own wait was refused");

    // AND A PROJECT CAN HAND IT BACK.
    let on = [("JBX_ALLOW_WAIT", "true")];
    let back = s.run_with(&on, &["wait", "j0000000", "--via-agent"]);
    assert_ne!(back.status.code(), Some(2), "`allow_wait: true` did not lift the refusal");
}

#[test]
fn the_hook_marks_only_a_bare_wait_and_only_where_monitor_exists() {
    let s = Scratch::new("markwait");

    let marked = hook(&s, "jbx wait j123abc");
    assert!(marked.contains("--via-agent"), "the agent's own wait went unmarked:\n{marked}");

    // COMPOUND LINES ARE LEFT ALONE. Appending a flag to `jbx wait x &&
    // deploy` changes what the shell runs. This is a guardrail against a
    // habit, not against somebody deliberately working around it.
    let compound = hook(&s, "jbx wait j123abc && echo done");
    assert!(!compound.contains("--via-agent"), "a compound line was rewritten:\n{compound}");

    // AND NEVER ON A CLIENT WITH NO MONITOR. Cursor has no end-of-turn
    // hook either, so `jbx wait` is the only way an ending reaches
    // anybody there; refusing it would silence the mechanism, not the
    // habit.
    let elsewhere = hook_as(&s, "cursor", "preToolUse", "Shell", "jbx wait j123abc");
    assert!(!elsewhere.contains("--via-agent"), "cursor lost its only channel:\n{elsewhere}");
}

#[test]
fn recognising_a_client_happens_in_one_place() {
    // THREE THINGS NEED TO KNOW WHICH HARNESS THIS IS — the mailbox
    // name, the wording of the detachment, and whether `allow_wait` has
    // anything to refuse — and each used to read the environment for
    // itself. Three readings of one fact is how they come to disagree:
    // one learns a new client and the others do not.
    //
    // So the environment is read in `harness.rs` and nowhere else. This
    // guard is the only thing that keeps it that way, because the next
    // person needing the answer will reach for `std::env::var` first.
    let mut elsewhere = Vec::new();
    for file in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap() {
        let path = file.unwrap().path();
        if path.file_name().is_some_and(|n| n == "harness.rs") {
            continue;
        }
        if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            // In code, not in prose: a comment may name the variable.
            for line in text.lines().filter(|l| !l.trim_start().starts_with("//")) {
                if line.contains("CLAUDE_CODE_SESSION_ID") {
                    elsewhere.push(format!("{}: {}", path.display(), line.trim()));
                }
            }
        }
    }
    assert!(elsewhere.is_empty(), "the environment is read outside harness.rs:\n{elsewhere:#?}");
}

#[test]
fn the_hook_knows_itself_however_it_is_spelled() {
    // IT DID NOT, ON WINDOWS. Installed as `jbx.exe` and written as
    // `jbx`, comparing whole file names made the two different tools —
    // so the hook wrapped its own commands there, which is the one thing
    // `is_us` exists to prevent. Linux never saw it: the two spellings
    // are the same word.
    let binary = if cfg!(windows) { r"C:\tools\jbx.exe" } else { "/usr/local/bin/jbx" };
    for written in ["jbx ps", "'jbx' ps", "/usr/local/bin/jbx ps", "./jbx ps"] {
        assert!(jobbox::hook::is_us(written, binary), "did not recognise itself in {written:?}");
    }
    assert!(!jobbox::hook::is_us("jbxtra ps", binary), "claimed a different tool");
    assert!(!jobbox::hook::is_us("cargo build", binary), "claimed somebody else's command");
}

#[test]
fn top_is_one_snapshot_when_nothing_can_be_redrawn() {
    // A LOOP THAT NEVER ENDS IS HOW A PIPE BECOMES A HANG. `top` redraws
    // for somebody watching; with no terminal there is nothing to redraw
    // into, so it answers once and leaves — which is also what keeps it
    // safe for anything that captures output, an agent included.
    let s = Scratch::new("top");
    let out = s.run(&["top"]);
    assert_eq!(out.status.code(), Some(0), "`top` did not come back");
    // The same table as `ps`, from the same renderer.
    assert_eq!(text(&out), text(&s.run(&["ps"])), "`top` and `ps` drew different tables");
}

#[test]
fn the_spans_say_what_the_percentage_is_a_percentage_of() {
    // "last hour … 25m24s saved (21%)" was read as a share of the hour,
    // by the person who wrote the tool. It is a share of the COMMAND
    // time — which on a machine running several agents at once is
    // routinely MORE than the window: two hours of commands inside one
    // hour of clock is ordinary here.
    //
    // A percentage next to a duration, on a row labelled by a stretch of
    // clock, cannot be read any other way unless its denominator is
    // printed beside it.
    // A DETACHED LINE, so there is something to report. A run that never
    // detaches leaves the table empty and this asserts nothing.
    // A DETACHED LINE, AND ITS ENDING WAITED FOR. Asking `gain` before
    // the reading is written answers "nothing measured yet", which is
    // true and tests nothing.
    let s = Scratch::new("gainspan");
    let said = text(&s.run(&["run", "--after", "1", "--", "sleep 3"]));
    let id = announced(&said);
    s.run(&["wait", &id]);
    let shown = text(&s.run(&["gain"]));
    // ANY OF THE THREE SPANS: which ones have something to say depends
    // on how long the store has existed, and the claim here is about the
    // shape of the row, not about the window.
    assert!(shown.contains(" saved of "), "no span row names its denominator:\n{shown}");
    assert!(shown.contains("never out of the clock"), "and nothing rules out the wrong reading");
}

#[test]
fn prune_forgets_what_is_over_and_leaves_what_is_happening() {
    // TWO KINDS DESERVE REMOVING AND NOTHING ELSE DOES: a job with an
    // exit code, and one whose record claims it is running while no
    // process answers. Anything alive is left exactly where it is,
    // however old and however quiet — age is not a fault, and the
    // thirty-five-minute job that prompted this verb was a harness's own
    // background loop, held on purpose.
    let s = Scratch::new("prune");
    s.run(&["run", "--after", "0", "--", "true"]);
    let running = text(&s.run(&["run", "--after", "0", "--", "sleep 30"]));
    let alive = announced(&running);
    until("the short one to finish", || {
        text(&s.run(&["list"])).contains("finished")
    });

    let said = text(&s.run(&["prune"]));
    assert!(said.contains("finished"), "the finished job was not forgotten:\n{said}");
    assert!(!said.contains(&alive), "prune touched a job that was still running:\n{said}");
    assert!(text(&s.run(&["ps"])).contains(&alive), "the running job disappeared");

    // AND A SECOND PRUNE FINDS NOTHING, which is what says it removed
    // rather than merely reported.
    assert!(text(&s.run(&["prune"])).contains("nothing to forget"));
}

#[test]
fn a_job_held_on_purpose_is_not_called_mute() {
    // `--after inf` is what the hook gives a line the harness is already
    // running in the background: detaching underneath it would make the
    // wrapper exit at the cut and the harness announce the work finished
    // when it had barely started. Such a job prints nothing for minutes
    // by design — an `until` loop has nothing to say until it is over —
    // and calling that MUTE raises an alarm about a chosen silence.
    let s = Scratch::new("heldjob");
    let quiet = [("JBX_MUTE_AFTER", "1")];
    // SPAWNED, NOT RUN. `--after inf` never lets go — that is the whole
    // property under test — so waiting for it here would wait out the
    // job and then look at an empty store.
    let mut held = Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .env("JBX_DIR", &s.0)
        .env("JBX_CONFIG", s.0.join("global.yaml"))
        .env("JBX_CLIENT", "me")
        .env("JBX_MUTE_AFTER", "1")
        .args(["run", "--after", "inf", "--", "sleep 30"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    until("the held job to appear", || text(&s.run_with(&quiet, &["ps"])).contains("sleep 30"));
    std::thread::sleep(std::time::Duration::from_secs(2));
    let shown = text(&s.run_with(&quiet, &["ps"]));
    let _ = held.kill();
    let _ = held.wait();
    assert!(shown.contains("held"), "a held job did not say so:\n{shown}");
    // NEITHER WORD: a held job that has printed nothing would otherwise be
    // called `NO OUTPUT`, and a guard that only looked for `MUTE` would stay
    // green with the `!r.held` exception gone — which the CI's mutation job
    // exists to catch.
    assert!(!shown.contains("MUTE") && !shown.contains("NO OUTPUT"),
            "a deliberate silence was called mute:\n{shown}");
}

#[test]
fn the_short_version_flag_works_and_a_flag_is_not_called_a_verb() {
    // `jbx -v` answered "unknown verb \"-v\"", which is wrong twice: the
    // short form is what everyone types, and a flag is not a verb. The
    // second half matters more — being told you invented a verb sends
    // you looking for the right verb, when what you needed was to put
    // the flag after one.
    let s = Scratch::new("shortv");
    for spelling in ["-v", "-V", "--version"] {
        let out = s.run(&[spelling]);
        assert_eq!(out.status.code(), Some(0), "`jbx {spelling}` did not answer");
        assert!(text(&out).starts_with("jbx "), "`jbx {spelling}` said something else");
    }
    let stray = s.run(&["-x"]);
    let said = String::from_utf8_lossy(&stray.stderr).into_owned();
    assert!(said.contains("is a flag"), "a flag was called a verb:\n{said}");
    assert!(!said.contains("unknown verb"), "still calls it a verb:\n{said}");
}

#[test]
fn kill_by_age_stops_the_old_and_never_its_own_caller() {
    // THE FLAGS THAT STOP BELONG TO THE VERB THAT STOPS. `prune` forgets;
    // `kill` is where an age threshold goes, because what it does is
    // stop things.
    let s = Scratch::new("killold");
    let young = announced(&text(&s.run(&["run", "--after", "0", "--", "sleep 30"])));
    std::thread::sleep(std::time::Duration::from_secs(3));

    // Nothing is three hours old, so nothing goes.
    assert!(text(&s.run(&["kill", "--too-old"])).contains("nothing has been running that long"));

    let said = text(&s.run(&["kill", "--older-than", "2s"]));
    assert!(said.contains(&young), "the old job was not stopped:\n{said}");
    // THE RECORD STAYS. A job you have just stopped is the one whose log
    // you are most likely to want; `prune` clears it afterwards.
    assert!(text(&s.run(&["list"])).contains(&young), "the record went with the process");

    // AN AGE IS WRITTEN THE WAY PEOPLE WRITE ONE, and anything else is
    // refused rather than guessed at.
    let bad = s.run(&["kill", "--older-than", "soon"]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("30s, 45m, 2h"));
}

#[test]
fn a_bulk_kill_spares_the_command_running_it() {
    // `jbx kill` IS ITSELF A WRAPPED COMMAND under a hook, so a wrapper
    // above it has a record like any other — and with a short age it is
    // old enough to match. Stopping it kills the kill half way through,
    // and the caller sees a command that died for no reason it can name.
    let mine = jobbox::store::ancestors();
    assert!(mine.contains(&std::process::id()), "we are not in our own ancestry");
    // ON WINDOWS THE CHAIN IS JUST US, on purpose: walking it there costs
    // a PowerShell query per step, which is seconds. A bulk kill will not
    // stop itself; it may stop the wrapper above it. Said here so the
    // weaker guarantee is a decision rather than a surprise.
    if !cfg!(windows) {
        assert!(mine.len() > 1, "the chain stopped at ourselves: {mine:?}");
    }
}

#[test]
fn a_two_day_old_record_whose_process_is_gone_is_collected_by_itself() {
    // THE ONE THING `prune` EXISTED FOR THAT NOTHING DID AUTOMATICALLY.
    // The daily sweep skipped every record without an exit code, so a
    // job whose process died without writing one sat in every listing
    // looking like work in progress, for ever.
    //
    // Bounded to records already two days old: asking whether a process is
    // alive costs a spawned `ps` on macOS, and doing that for every
    // record on every wrapped command would be a tax on the healthy to
    // catch the rare.
    let s = Scratch::new("selfcollect");
    s.run(&["run", "--after", "0", "--", "true"]);
    until("the job to finish", || text(&s.run(&["list"])).contains("finished"));
    let id = text(&s.run(&["list"]))
        .lines()
        .find_map(|l| l.split_whitespace().next().filter(|w| w.starts_with('j')).map(str::to_string))
        .expect("a job");

    // Make it look like a day-old record of a job that never landed: no
    // exit code, and a pid nothing answers to.
    let record = s.jobs().join(format!("{id}.json"));
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
    // FIFTY HOURS: past the forty-eight a codeless record is given, which
    // is twice the day a finished one gets — being wrong about a job
    // that is still running deletes the only trace of live work.
    v["started"] = serde_json::json!(v["started"].as_f64().unwrap() - 50.0 * 3600.0);
    v["pid"] = serde_json::json!(4_000_000_000u64);
    std::fs::write(&record, v.to_string()).unwrap();
    std::fs::remove_file(s.jobs().join(format!("{id}.code"))).unwrap();
    assert!(record.exists());

    // Any wrapped command sweeps; nothing has to be asked for.
    s.run(&["run", "--after", "0", "--", "true"]);
    assert!(!record.exists(), "the two-day-old record with no process survived the sweep");
}

#[test]
fn the_listing_verbs_explain_their_state_column() {
    // SIX WORDS THAT LOOK ALIKE AND ARE NOT, and the difference decides
    // what to do next: `foreground` is somebody standing still, `held`
    // is somebody who chose to, `background` is nobody. Guessing between
    // them is how a healthy listing comes to look frightening — which is
    // how a thirty-five-minute `held` job was read as a hang.
    let s = Scratch::new("stateshelp");
    for verb in ["ps", "top", "list"] {
        let said = text(&s.run(&[verb, "--help"]));
        for state in ["queued", "foreground", "background", "held", "gone", "MUTE", "NO OUTPUT"] {
            assert!(said.contains(state), "`jbx {verb} --help` never mentions `{state}`:\n{said}");
        }
        // SAID ONCE, FROM ONE PLACE. Three copies of an explanation is
        // two that will not be updated.
        assert!(said.contains("the state column"), "`jbx {verb} --help` lost the block");
    }
    // AND NOT EVERYWHERE. A note on every verb is a note the eye skips.
    assert!(!text(&s.run(&["kill", "--help"])).contains("the state column"));
}

#[test]
fn a_line_the_harness_backgrounded_is_neither_stood_through_nor_saved() {
    // IT WAS FILED AS STOOD STILL. A line the harness already runs in its
    // own background reaches jbx as `--after inf`; JSON writes that cut as
    // `null`, and `gain` read it back as no cut at all — so every second
    // of a background loop went into `waited`, and an hour of polling read
    // as an hour of the caller standing still. Through the real command,
    // because the `null` only exists on the way through the file.
    let s = Scratch::new("heldgain");
    s.run(&["run", "--after", "inf", "--", "sleep 3"]);
    s.run(&["run", "--", "echo quick"]);
    // AND A DELIBERATE FOREGROUND, whose cut is infinite as well. The first
    // version of this fix told the two apart by the cut alone, and the
    // count of foregrounds chosen on purpose went quietly to zero.
    s.run(&["fg", "--", "echo chosen"]);
    let seen = readings(&s);
    assert_eq!(
        seen.iter().filter(|r| r["kind"] == "run" && r["after"].is_null()).count(), 2,
        "the held line and the foreground were not both written with an infinite cut: {seen:?}"
    );

    let doc: serde_json::Value =
        serde_json::from_str(&text(&s.run(&["gain", "--json"]))).expect("gain is JSON");
    let total = &doc["total"];
    assert_eq!(total["calls"], 3, "every line is a call: {total}");
    assert_eq!(total["held"], 1, "{total}");
    assert_eq!(total["chosen"], 1, "the deliberate foreground was taken for a held line: {total}");
    assert!(total["held_secs"].as_f64().unwrap() >= 3.0, "{total}");
    assert!(total["waited"].as_f64().unwrap() < 2.0,
            "the background line was counted as stood through: {total}");
    assert!(total["elapsed"].as_f64().unwrap() < 2.0,
            "the background line was counted as jbx's to give back: {total}");
    // AND NO CUT IS CREDITED WITH IT. Replayed at 2s, three seconds of
    // `sleep` would read as a detach and a saving that no threshold could
    // ever have produced for a line that was never going to be cut.
    for row in doc["thresholds"].as_array().expect("thresholds replayed") {
        assert_eq!(row["would_detach"], 0, "a held line was replayed as cut: {row}");
    }

    let shown = text(&s.run(&["gain"]));
    assert!(shown.contains("1 of 3 calls already ran in the harness's background"),
            "the held line is not accounted for anywhere visible:\n{shown}");
    assert!(shown.contains("1 of 3 calls asked for the foreground on purpose"),
            "the deliberate foreground is no longer counted:\n{shown}");
}

/// THE SUPERVISOR OF A JOB, from the record `queue` wrote for it.
#[cfg(unix)]
fn supervisor_pid(s: &Scratch, id: &str) -> u32 {
    let raw = std::fs::read_to_string(s.jobs().join(format!("{id}.json")))
        .unwrap_or_else(|e| panic!("no record for {id:?}: {e}"));
    let record: serde_json::Value = serde_json::from_str(&raw).unwrap();
    record["pid"].as_u64().expect("a record with a pid") as u32
}

/// When this machine came up, or `None` where nothing here asks.
///
/// ONLY LINUX GUARDS ON IT, so only Linux needs to respect it: on the
/// other systems `store::is_ours` cannot read a boot time and says so by
/// answering yes, and a backdated record stays listed.
#[cfg(target_os = "linux")]
fn booted_at() -> Option<f64> {
    std::fs::read_to_string("/proc/stat")
        .ok()?
        .lines()
        .find_map(|line| line.strip_prefix("btime "))
        .and_then(|value| value.trim().parse().ok())
}

#[cfg(not(target_os = "linux"))]
fn booted_at() -> Option<f64> {
    None
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// GONE WITHIN THE DEADLINE — or stopped by force, so that a failing test
/// does not leave a supervisor spinning on the machine running the suite.
#[cfg(unix)]
fn gone_within(pid: u32, secs: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while std::time::Instant::now() < deadline {
        if !pid_alive(pid) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    false
}

#[test]
#[cfg(unix)]
fn a_queued_job_that_cannot_take_a_ticket_gives_up_instead_of_spinning() {
    // FOUND AT 94% CPU FOR TWENTY MINUTES on a machine busy with other
    // work, and reproduced at 77%: `take_ticket` retried the same
    // impossible write with no pause and no end once its directory was
    // gone. Here a file sits where that directory must be, so no ticket
    // can ever be written.
    let s = Scratch::new("ticketless");
    let slots = s.jobs().join("slots");
    std::fs::create_dir_all(&slots).unwrap();
    std::fs::write(slots.join("tickets"), "not a directory").unwrap();
    let one = [("JBX_SLOTS", "1")];
    let said = text(&s.run_with(&one, &["queue", "no-ticket", "--", "sleep 3"]));
    let id = said.lines().next().unwrap_or("").trim().to_string();
    let pid = supervisor_pid(&s, &id);
    assert!(gone_within(pid, 10),
            "the supervisor of {id} was still running 10s after it could not queue");
}

#[test]
#[cfg(unix)]
fn a_queued_job_stops_waiting_when_its_cache_is_deleted() {
    // USAGE SAYS `cache/` IS SAFE TO DELETE. Deleted while a job waited its
    // turn, the supervisor waited for ever for a slot in a directory that
    // was no longer there — reproduced alive twelve seconds after the job
    // ahead of it had finished. It is also how this suite left orphans on
    // a machine: a test's scratch directory went while its queued jobs
    // still waited.
    let s = Scratch::new("cache-gone");
    let one = [("JBX_SLOTS", "1")];
    let first_line = |out: &Output| text(out).lines().next().unwrap_or("").trim().to_string();
    let holder = first_line(&s.run_with(&one, &["queue", "holder", "--", "sleep 6"]));
    // THE HOLDER MUST REALLY HOLD THE SLOT, or the waiter could take it
    // first, run its second, and exit — passing this test without waiting.
    until("the holder to start", || {
        std::fs::read_dir(s.jobs())
            .map(|d| d.flatten().any(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.starts_with(&holder) && name.contains("start")
            }))
            .unwrap_or(false)
    });
    let waiter = first_line(&s.run_with(&one, &["queue", "waiter", "--", "sleep 1"]));
    let holder_pid = supervisor_pid(&s, &holder);
    let waiter_pid = supervisor_pid(&s, &waiter);

    // DELETED UNTIL IT IS GONE, not once. A supervisor can create a file in
    // it while it is being removed — the ticket of the waiter, the start of
    // the holder — and a single `remove_dir_all` then fails with "Directory
    // not empty", which is how this test first failed, on nothing to do with
    // what it checks.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while s.jobs().exists() && std::time::Instant::now() < deadline {
        let _ = std::fs::remove_dir_all(s.jobs());
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if s.jobs().exists() {
        let _ = Command::new("kill").args(["-9", &holder_pid.to_string()]).status();
        let _ = Command::new("kill").args(["-9", &waiter_pid.to_string()]).status();
        panic!("the cache could not be deleted within 2s");
    }
    let gone = gone_within(waiter_pid, 10);
    let _ = Command::new("kill").args(["-9", &holder_pid.to_string()]).status();
    assert!(gone, "the waiting supervisor of {waiter} outlived its deleted cache by 10s");
}

#[test]
fn every_verb_answers_help_with_its_own_usage() {
    // TWO OF TWENTY-SIX DID NOT. `hook` took `--help` for a client's name
    // and `queue` for an intent with no line: both read their arguments by
    // hand, past the one place that answers help for every other verb. A
    // new verb parsed the same way would fail the same way, and nobody
    // would know until somebody asked it — so every verb in the table is
    // asked, from a HOME of the test's own.
    let s = Scratch::new("help-all");
    let home = s.0.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let home = home.to_str().unwrap();
    let env = [("HOME", home), ("USERPROFILE", home)];
    for v in jobbox::describe::VERBS {
        let out = s.run_with(&env, &[v.name, "--help"]);
        let said = text(&out);
        assert_eq!(out.status.code(), Some(0),
                   "`jbx {} --help` failed:\n{said}{}", v.name, String::from_utf8_lossy(&out.stderr));
        assert!(said.starts_with(&format!("jbx {} —", v.name)),
                "`jbx {} --help` did not print its usage:\n{said}", v.name);
    }
    // AND ASKING DID NOTHING ELSE: no job was queued or run on the way.
    let left: Vec<_> = std::fs::read_dir(s.jobs())
        .map(|d| d.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "asking for help left jobs behind: {left:?}");
}

#[test]
fn no_listing_row_outruns_its_width_when_the_cells_grow_long() {
    // THE WIDTHS WERE WRITTEN DOWN AND THE CELLS WERE NOT. `held
    // 12177s` is seventeen characters in a sixteen-character column,
    // `finished  exit 127` eighteen, and a project named past fourteen
    // letters adds the rest; `format!` cuts none of them. A job held for
    // three hours drew every row of `ps` one character past the terminal,
    // and measured with a long project name, thirteen. The width guard
    // in `a_job_is_named_by_whoever_ran_it_when_they_said` never saw it:
    // its jobs are seconds old and run from a short-named directory.
    let s = Scratch::new("pscells");
    let project = s.0.join("a-project-with-a-long-name");
    std::fs::create_dir_all(&project).unwrap();
    let jbx_in_project = |args: &[&str]| {
        Command::new(JBX)
            .env_remove("JBX_WRAPPED")
            .env("JBX_DIR", &s.0)
            .env("JBX_CONFIG", s.0.join("global.yaml"))
            .env("JBX_CLIENT", "me")
            .current_dir(&project)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    let long_line = format!("sleep 60; echo {}", "x".repeat(200));
    let mut held = jbx_in_project(&["run", "--after", "inf", "--intent", "held for hours", "--", &long_line]);
    let mut failed = jbx_in_project(&["run", "--after", "0", "--", "exit 127"]);
    let _ = failed.wait();
    let records = || -> Vec<std::path::PathBuf> {
        std::fs::read_dir(s.jobs())
            .map(|d| d.flatten().map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json")).collect())
            .unwrap_or_default()
    };
    let codes = || {
        std::fs::read_dir(s.jobs())
            .map(|d| d.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "code")).count())
            .unwrap_or(0)
    };
    until("both jobs to be recorded, and the failing one to end", || records().len() == 2 && codes() == 1);

    // THREE HOURS OLD, without waiting three hours — BUT NEVER OLDER
    // THAN THE MACHINE. Three hours used to be written here flat, and a
    // record that predates the boot names a process that cannot exist:
    // `jbx` reads such a record as `gone` and stops listing it, which on
    // a runner a few minutes old took this test with it rather than the
    // widths it is about. The age is borrowed from the uptime when the
    // uptime is shorter, and the widest state cell in the table —
    // `finished  exit 127`, eighteen characters against a sixteen-wide
    // column — belongs to the finished job and does not depend on it.
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    let aged = (now - 12177.0).max(booted_at().map_or(f64::MIN, |up| up + 1.0));
    for path in records() {
        let mut record: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        if record["held"] == true {
            record["started"] = serde_json::json!(aged);
            std::fs::write(&path, record.to_string()).unwrap();
        }
    }

    for verb in ["ps", "list"] {
        for wide in [80usize, 100, 137, 200] {
            let drawn = text(&s.run(&[verb, "--all", "--width", &wide.to_string()]));
            // LISTED AT ALL, which is what the widths are measured on:
            // a table that drew nothing would pass every check below.
            // The age itself is not asserted — it is whatever the
            // machine's uptime allowed above.
            assert!(drawn.contains("held ") || verb == "list",
                    "the aged held job is not listed:\n{drawn}");
            for row in drawn.lines() {
                assert!(row.chars().count() <= wide,
                        "`{verb}` drew a row {} past a {wide}-column table:\n{row}",
                        row.chars().count() - wide);
            }
        }
    }
    let _ = held.kill();
    let _ = held.wait();
    s.stop_everything();
}

/// `jbx hook --list` against a HOME of the test's making. The client's own
/// override is removed as well: `CLAUDE_CONFIG_DIR` set on the machine
/// running the suite would send the listing to its real settings.
fn hook_list_with_home(home: &std::path::Path, s: &Scratch, args: &[&str]) -> Output {
    Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("JBX_DIR", &s.0)
        .env("JBX_CONFIG", s.0.join("global.yaml"))
        .args(["hook", "--list"])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn listed_client<'a>(list: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    list.as_array().unwrap().iter().find(|c| c["name"] == name).unwrap()
}

fn declare_hook_in(home: &std::path::Path, dir: &str, event: &str, command: &str) {
    std::fs::create_dir_all(home.join(dir)).unwrap();
    let settings = serde_json::json!({
        "hooks": { event: [{ "matcher": "", "hooks": [{ "type": "command", "command": command }] }] }
    });
    std::fs::write(home.join(dir).join("settings.json"), settings.to_string()).unwrap();
}

#[test]
fn hook_list_says_whether_the_hook_is_declared_and_where_it_points() {
    // IT NAMED THE FILE AND SAID NOTHING OF WHAT WAS IN IT. On 10/09 the
    // hook pointed at a jbx two versions behind the published one, and
    // nothing said so — "gain looks broken" is what found it.
    let s = Scratch::new("hooklist");
    let home = s.0.join("home");
    declare_hook_in(&home, ".claude", "PreToolUse", &format!("{JBX} hook claude"));
    std::fs::create_dir_all(home.join(".gemini")).unwrap();

    let list: serde_json::Value =
        serde_json::from_str(&text(&hook_list_with_home(&home, &s, &["--json"])))
            .expect("hook --list --json is JSON");
    let claude = &listed_client(&list, "claude")["hook"];
    assert_eq!(claude["declared"], true, "{claude}");
    assert_eq!(claude["is_this_binary"], true, "{claude}");
    assert_eq!(claude["version"], env!("CARGO_PKG_VERSION"), "{claude}");
    assert_eq!(listed_client(&list, "gemini")["hook"]["declared"], false, "{list}");
    // NOT CHECKABLE IS SAID, not guessed: jbx knows no file for cursor.
    assert_eq!(listed_client(&list, "cursor")["hook"]["checkable"], false, "{list}");

    // A HOOK POINTING AT NOTHING fails on every command the agent runs.
    let gone = s.0.join("gone").join("jbx");
    declare_hook_in(&home, ".gemini", "BeforeTool", &format!("{} hook gemini", gone.display()));
    let list: serde_json::Value =
        serde_json::from_str(&text(&hook_list_with_home(&home, &s, &["--json"]))).unwrap();
    let gemini = &listed_client(&list, "gemini")["hook"];
    assert_eq!(gemini["declared"], true, "{gemini}");
    assert_eq!(gemini["exists"], false, "{gemini}");
}

#[test]
#[cfg(unix)]
fn hook_list_names_another_version_and_does_not_wait_on_a_binary_that_hangs() {
    // THE TWO CASES A REAL INSTALL PRODUCES: an older copy still declared,
    // and a binary that never answers — which is what a half-written one
    // did to a whole session once. The listing must say both, and must not
    // hang on the second.
    use std::os::unix::fs::PermissionsExt;
    let s = Scratch::new("hooklist-other");
    let home = s.0.join("home");
    let fake = |dir: &str, body: &str| {
        let path = s.0.join(dir).join("jbx");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    let older = fake("older", "echo 'jbx 0.0.1'");
    let stuck = fake("stuck", "sleep 30");
    declare_hook_in(&home, ".claude", "PreToolUse", &format!("{} hook claude", older.display()));
    declare_hook_in(&home, ".gemini", "BeforeTool", &format!("{} hook gemini", stuck.display()));

    let began = std::time::Instant::now();
    let list: serde_json::Value =
        serde_json::from_str(&text(&hook_list_with_home(&home, &s, &["--json"]))).unwrap();
    assert!(began.elapsed() < std::time::Duration::from_secs(15),
            "--list waited on a binary that never answers: {:?}", began.elapsed());
    let claude = &listed_client(&list, "claude")["hook"];
    assert_eq!(claude["version"], "0.0.1", "{claude}");
    assert_eq!(claude["same_version"], false, "{claude}");
    assert_eq!(claude["is_this_binary"], false, "{claude}");
    let gemini = &listed_client(&list, "gemini")["hook"];
    assert_eq!(gemini["exists"], true, "{gemini}");
    assert!(gemini["version"].is_null(), "a binary that never answered got a version: {gemini}");

    // AND THE PERSON READING IT IS TOLD, in words.
    let said = text(&hook_list_with_home(&home, &s, &[]));
    assert!(said.contains("answers jbx 0.0.1, not this binary"), "{said}");
    assert!(said.contains("did not answer --version"), "{said}");
}

#[test]
fn the_hook_gives_the_lines_it_rewrites_no_input() {
    // AN AGENT NEVER TYPES INTO A COMMAND, and the input a harness hands
    // its shell tool is not at end of file: under Claude Code it is a
    // socket that never closes. The hook says so on every line it rewrites.
    let s = Scratch::new("noinput-hook");
    let plain = s.project(None, "src");
    let rewritten = ask_hook(&plain, &s, "");
    assert!(rewritten.contains("run --no-input "),
            "the rewrite leaves the line an input to wait on: {rewritten}");
}

#[test]
fn a_line_given_no_input_does_not_wait_for_input_that_never_comes() {
    // MEASURED UNDER CLAUDE CODE (#2271): the shell tool's standard input
    // is a socket nobody closes, so `read` waited for ever — and once the
    // line detached, nothing was left to time it out. A pipe this test
    // holds open and never writes to is the same situation.
    use std::io::Read;
    let s = Scratch::new("noinput");
    let mut child = Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .env("JBX_DIR", &s.0)
        .env("JBX_CONFIG", s.0.join("global.yaml"))
        .args(["run", "--no-input", "--after", "30", "--", "read -r x; echo read-exit=$?"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // HELD, NEVER WRITTEN, NEVER CLOSED — until the test is over.
    let open = child.stdin.take();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            drop(open);
            panic!("the line waited 20s for input nobody will ever type");
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    drop(open);
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    assert!(out.contains("read-exit=1"), "the line did not find its input closed: {out}");
}

#[test]
fn a_line_run_by_hand_still_reads_what_is_piped_into_it() {
    // WITHOUT THE FLAG NOTHING CHANGES. `cat f | jbx run -- sort` is a
    // person's pipeline, and a wrapper closing its input would be a
    // wrapper altering the command it wraps.
    use std::io::Write;
    let s = Scratch::new("pipein");
    let mut child = Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .env("JBX_DIR", &s.0)
        .env("JBX_CONFIG", s.0.join("global.yaml"))
        .args(["run", "--", "cat"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"piped through\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(text(&out).contains("piped through"),
            "the piped input never reached the line: {}", text(&out));
}

#[test]
fn lines_appended_by_many_writers_at_once_all_come_back_whole() {
    // TWO ENDINGS FINISHING TOGETHER CAME OUT AS ONE UNREADABLE LINE.
    // `writeln!` on a file is one write per formatted piece — per JSON
    // token for a `serde_json::Value` — and appends interleave between
    // writes. Twenty jobs ending at once lost 5 endings in 160 and left 17
    // measurements in 200 unreadable. Sixteen writers hammering one file is
    // the same race, made likely enough to be caught every time.
    let s = Scratch::new("append");
    std::fs::create_dir_all(s.home()).unwrap();
    let file = s.home().join("many.jsonl");
    let writers: Vec<_> = (0..16)
        .map(|w| {
            let file = file.clone();
            std::thread::spawn(move || {
                for i in 0..200 {
                    let line = serde_json::json!({ "writer": w, "i": i, "pad": "x".repeat(120) });
                    jobbox::store::append_line(&file, &line.to_string());
                }
            })
        })
        .collect();
    for w in writers {
        w.join().unwrap();
    }
    let content = std::fs::read_to_string(&file).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    let whole: std::collections::BTreeSet<(u64, u64)> = lines
        .iter()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| Some((v["writer"].as_u64()?, v["i"].as_u64()?)))
        .collect();
    assert_eq!(lines.len(), 3200, "lines were glued together or lost");
    assert_eq!(whole.len(), 3200, "{} of 3200 lines came back readable", whole.len());
}

#[test]
fn jobs_ending_together_leave_every_ending_and_every_measurement_readable() {
    // THE WIRING, since the race lives between processes. Eight
    // supervisors finish within the same instant and each appends an
    // ending and a measurement; a line split across writes comes back
    // glued to its neighbour and parses as neither. It was found this
    // way: a CI test that ends two jobs together waited 20s for two
    // endings and saw one.
    let s = Scratch::new("together");
    let jobs: Vec<_> = (0..8)
        .map(|_| {
            Command::new(JBX)
                .env_remove("JBX_WRAPPED")
                .env("JBX_DIR", &s.0)
                .env("JBX_CONFIG", s.0.join("global.yaml"))
                .env("JBX_CLIENT", "me")
                .args(["run", "--after", "0", "--", "sleep 1"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for mut job in jobs {
        let _ = job.wait();
    }
    // THE EXIT CODE IS WRITTEN LAST, so once eight are on disk the
    // endings and the measurements written before them are too.
    let codes = || {
        std::fs::read_dir(s.jobs())
            .map(|d| {
                d.flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "code"))
                    .count()
            })
            .unwrap_or(0)
    };
    until("the eight jobs to finish", || codes() == 8);

    let endings: std::collections::BTreeSet<String> = signals_of(&s)
        .iter()
        .filter_map(|e| e["id"].as_str().map(str::to_string))
        .collect();
    assert_eq!(endings.len(), 8, "endings came back unreadable, or not at all");
    let raw = std::fs::read_to_string(s.home().join("readings.jsonl")).unwrap_or_default();
    let unreadable: Vec<&str> = raw
        .lines()
        .filter(|l| !l.trim().is_empty() && serde_json::from_str::<serde_json::Value>(l).is_err())
        .collect();
    assert!(unreadable.is_empty(), "measurements came back glued: {unreadable:?}");
}

#[test]
fn a_window_counts_only_the_part_of_a_line_that_fell_inside_it() {
    // A READING IS WRITTEN WHEN ITS LINE ENDS, and the windows used to be
    // chosen by that end alone — so a long line that merely finished in
    // the last hour brought all of its duration into it. Replayed on a
    // real store, the hour after a long detached build read 48% saved
    // where the hour itself held 13%.
    //
    // WRITTEN BY HAND, because the case needs a line two hours long.
    let s = Scratch::new("gainclip");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    // Two hours, detached at 30s, finished a minute ago: it began 7260s
    // ago, so the last hour holds its final 3540s, every one given back.
    // And a `jbx wait` of ten minutes that ended 55 minutes ago: five of
    // its minutes fall inside the hour.
    let seeded = format!(
        "{{\"at\":{a},\"kind\":\"run\",\"fg\":false,\"project\":\"clip\",\"path\":\"/clip\",\"shape\":\"make\",\"secs\":7200.0,\"after\":30.0,\"code\":0}}\n\
         {{\"at\":{w},\"kind\":\"wait\",\"project\":\"clip\",\"path\":\"/clip\",\"secs\":600.0}}\n\
         {{\"at\":{t},\"kind\":\"run\",\"fg\":false,\"project\":\"clip\",\"path\":\"/clip\",\"shape\":\"make\",\"secs\":100.0,\"after\":30.0,\"code\":0}}\n",
        a = now - 60.0,
        w = now - 3300.0,
        t = now - 3.0 * 86400.0,
    );
    std::fs::create_dir_all(s.home()).unwrap();
    std::fs::write(s.home().join("readings.jsonl"), seeded).unwrap();

    let doc: serde_json::Value =
        serde_json::from_str(&text(&s.run(&["gain", "--json"]))).expect("gain is JSON");
    let span = |name: &str| {
        doc["spans"].as_array().unwrap().iter().find(|x| x["span"] == name).unwrap().clone()
    };
    let near = |v: &serde_json::Value, key: &str, want: f64| {
        let got = v[key].as_f64().unwrap();
        assert!((got - want).abs() < 5.0, "{key} was {got}, not about {want}: {v}");
    };

    let hour = span("hour");
    near(&hour, "elapsed", 3540.0);
    // None of the line's first thirty seconds, five minutes of the wait.
    near(&hour, "waited", 300.0);
    assert_eq!(hour["calls"], 1, "the call is counted whole, where it ended: {hour}");
    assert_eq!(hour["detached"], 1, "{hour}");

    // A DAY HOLDS ALL OF IT, so nothing is cut there.
    let day = span("day");
    near(&day, "elapsed", 7200.0);
    near(&day, "waited", 630.0);

    // A WEEK HOLDS WHAT A DAY DOES NOT: a line that ended three days ago,
    // whole, next to the two above.
    let week = span("week");
    near(&week, "elapsed", 7300.0);
    near(&week, "waited", 660.0);
    assert_eq!(week["calls"], 2, "{week}");

    // AND `--since` IS THE SAME WINDOW, cut the same way.
    let since: serde_json::Value =
        serde_json::from_str(&text(&s.run(&["gain", "--since", "1h", "--json"])))
            .expect("gain is JSON");
    near(&since["total"], "elapsed", 3540.0);
    near(&since["total"], "waited", 300.0);
}

#[test]
fn gain_reset_forgets_this_project_by_path_and_keeps_the_others() {
    // BY PATH, NOT BY NAME: two repositories can both be called `bms`,
    // and a reset aimed at one must not take the other's history.
    let s = Scratch::new("gainreset");
    s.run(&["run", "--after", "0", "--", "true"]);
    until("a reading to be written", || !readings(&s).is_empty());

    let foreign = r#"{"at":1788000000,"kind":"run","project":"other","path":"/somewhere/else","secs":3,"code":0}"#;
    let file = s.home().join("readings.jsonl");
    // NOT `text`: that name is the harness's helper, and a local binding
    // shadowing it breaks every `text(&...)` call below it.
    let mut content = std::fs::read_to_string(&file).unwrap();
    content.push_str(foreign);
    content.push('\n');
    std::fs::write(&file, content).unwrap();

    // A PROJECT NAMED ALONGSIDE IT IS REFUSED, not ignored — quietly
    // resetting this one instead erases the history somebody meant to keep.
    let named = s.run(&["gain", "other", "--reset"]);
    assert_eq!(named.status.code(), Some(2), "a named project was not refused");
    assert_eq!(readings(&s).len(), 2, "the refused reset touched the file anyway");

    let said = text(&s.run(&["gain", "--reset"]));
    assert!(said.contains("forgot 1 reading "), "it did not say what it forgot:\n{said}");
    let left = readings(&s);
    assert_eq!(left.len(), 1, "this project's reading survived, or the other's went");
    assert_eq!(left[0]["path"], "/somewhere/else", "the wrong project's history was kept");

    // AND `--all` IS EVERYTHING.
    s.run(&["gain", "--reset", "--all"]);
    assert!(readings(&s).is_empty(), "`--all` left readings behind");
}

/// A record whose pid the machine has since handed to somebody else.
///
/// FABRICATED FROM A REAL ONE, taking the exit code back out: a job that
/// died with the machine never wrote one, and it is the absence of a
/// code that makes the pid the only thing left to read.
#[cfg(target_os = "linux")]
fn hand_the_record_to(s: &Scratch, pid: u32, started: f64) -> String {
    s.run(&["run", "--after", "0", "--", "true"]);
    until("the job to finish", || text(&s.run(&["list"])).contains("finished"));
    let id = text(&s.run(&["list"]))
        .lines()
        .find_map(|l| l.split_whitespace().next().filter(|w| w.starts_with('j')).map(str::to_string))
        .expect("a job");
    let record = s.jobs().join(format!("{id}.json"));
    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
    v["pid"] = serde_json::json!(pid);
    v["started"] = serde_json::json!(started);
    std::fs::write(&record, v.to_string()).unwrap();
    let _ = std::fs::remove_file(s.jobs().join(format!("{id}.code")));
    id
}

#[test]
#[cfg(target_os = "linux")]
fn a_job_the_machine_outlived_is_gone_and_its_pid_is_left_alone() {
    // WHAT A REBOOT LEAVES. The supervisor goes down with the machine
    // without writing an exit code, so the record's pid is all a reader
    // has — and the kernel is free to hand that number to somebody else,
    // which it does. Seen for real: a job read `background 73785s`
    // twenty hours after its work had finished, because its pid had been
    // given to a container shim that started after the reboot.
    //
    // The `sleep` here stands in for the shim. If the guard goes, this
    // test does not merely fail: the signal lands on it.
    let s = Scratch::new("rebooted");
    let mut stranger = Command::new("sleep")
        .arg("60")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("a process to stand in for the one that got the pid");
    let taken = stranger.id();

    // BEFORE THE MACHINE CAME UP, which is the whole claim: nothing
    // running now can be this job's, whatever `/proc` says about the
    // number it left behind.
    let id = hand_the_record_to(&s, taken, 1.0);

    let state = text(&s.run(&["status", &id]));
    assert!(state.contains("gone"),
            "a job older than the boot still reads as live:\n{state}");

    // AND NOTHING IS SIGNALLED — asking twice, because `--force` skips
    // the polite half and would otherwise reach the stranger first.
    for how in [vec!["kill", &id], vec!["kill", "--force", &id]] {
        let said = text(&s.run(&how));
        assert!(said.contains("nothing was signalled"),
                "`jbx {}` claimed to act on a pid that is not ours: {said}", how.join(" "));
        assert!(pid_alive(taken),
                "`jbx {}` signalled a process that only inherited the number", how.join(" "));
    }

    let _ = stranger.kill();
    let _ = stranger.wait();
}

#[test]
#[cfg(target_os = "linux")]
fn a_pid_that_names_a_thread_of_something_else_is_not_a_running_job() {
    // THE OTHER HALF OF THE SAME MISTAKE, and the one that made the
    // first invisible: `/proc/<pid>` answers for a THREAD as readily as
    // for a process, so `ps -p` printed nothing for that shim while this
    // program read it as alive. Every job jbx starts is its own process.
    //
    // The thread borrowed here is one of this test binary's own, so the
    // record points at a number that exists, is not a zombie, and is
    // still not a job. Nothing is signalled at it — deliberately: if
    // this guard ever goes, a `kill` here would take the suite with it.
    let s = Scratch::new("threadpid");
    let me = std::process::id();
    let tid: u32 = std::fs::read_dir("/proc/self/task")
        .expect("this process has threads")
        .filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .find(|t| *t != me)
        .expect("a thread that is not the process itself");

    let id = hand_the_record_to(&s, tid, jobbox_now());
    let state = text(&s.run(&["status", &id]));
    assert!(state.contains("gone"),
            "a pid naming a thread of another program read as a running job:\n{state}");
}

/// Now, the way a record spells it.
#[cfg(target_os = "linux")]
fn jobbox_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

#[test]
#[cfg(unix)]
fn force_stops_a_job_without_asking_it_to_stop_first() {
    // WHAT `--force` IS FOR: `kill` asks with TERM and waits a second
    // before insisting. That second is the wrong default for a line that
    // is never going to answer, and the flag is how somebody says so.
    let s = Scratch::new("killforce");
    let out = s.run(&["run", "--after", "0.1", "--", "sleep 30"]);
    let id = announced(&text(&out));
    let pid = supervisor_pid(&s, &id);

    let said = text(&s.run(&["kill", "--force", &id]));
    assert!(said.contains("stopped"), "`kill --force` did not report a stop: {said}");
    assert!(said.contains("not asked first"),
            "`kill --force` reported an ordinary stop: {said}");
    assert!(gone_within(pid, 5), "the supervisor outlived `kill --force`");
    s.stop_everything();
}
