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

    fn jobs(&self) -> PathBuf {
        self.0.join("jobs")
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
    let out = s.run(&["run", "--", "echo bonjour"]);
    assert_eq!(text(&out), "bonjour\n");
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
    assert!(said.contains("detached as j"), "not announced: {said}");
    assert_eq!(out.status.code(), Some(0), "detaching is not a failure");

    let id = said
        .split("detached as ")
        .nth(1)
        .and_then(|r| r.split('.').next())
        .unwrap()
        .trim()
        .to_string();

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
    assert!(said.contains("detached as j"));
}

#[test]
fn a_detached_line_keeps_its_log() {
    let s = Scratch::new("log");
    let said = text(&s.run(&["run", "--after", "1", "--", "echo one; sleep 2; echo two"]));
    let id = said.split("detached as ").nth(1).unwrap().split('.').next().unwrap().trim().to_string();
    s.run(&["wait", &id]);
    let log = text(&s.run(&["tail", &id]));
    assert!(log.contains("one") && log.contains("two"), "log lost half of it: {log:?}");
}

// ── THE HOOK ────────────────────────────────────────────────────────────

fn hook(s: &Scratch, command: &str) -> String {
    let event = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": command, "description": "d", "timeout": 120000},
    })
    .to_string();
    let mut child = Command::new(JBX)
        .env_remove("JBX_WRAPPED")
        .arg("hook")
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
    let tricky = r#"echo "c'est l'ete""#;
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
    assert!(declared.contains("jbx hook"), "declared the wrong binary: {declared}");
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

fn readings(s: &Scratch) -> Vec<serde_json::Value> {
    std::fs::read_to_string(s.jobs().join("stats.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

#[test]
fn every_line_leaves_a_reading_even_the_short_ones() {
    let s = Scratch::new("stats");
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
    let raw = std::fs::read_to_string(s.jobs().join("stats.jsonl")).unwrap();
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
    let id = said.split("detached as ").nth(1).unwrap().split('.').next().unwrap().trim().to_string();
    s.run(&["wait", &id]);

    let blocks: f64 = readings(&s)
        .iter()
        .filter(|r| r["kind"] == "wait")
        .filter_map(|r| r["secs"].as_f64())
        .sum();
    assert!(blocks > 1.0, "the block was not written down: {blocks}");

    let shown = text(&s.run(&["stats"]));
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
    let shown = text(&s.run(&["stats"]));
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
    let id = said.split("detached as ").nth(1).unwrap().split('.').next().unwrap().trim().to_string();
    s.run_as("me", &["wait", &id]);

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
    let id = said.split("detached as ").nth(1).unwrap().split('.').next().unwrap().trim().to_string();
    s.run_as("me", &["wait", &id]);
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
    let id = said.split("detached as ").nth(1).unwrap().split('.').next().unwrap().trim().to_string();
    s.run_as("them", &["wait", &id]);

    // ANNOUNCING IS ONE THING, BLOCKING IS ANOTHER. Blocking holds a
    // session open and sends the model to fix something; doing that for
    // a job another session started sends an agent to read a log from a
    // project it is not working on. Measured the day it happened.
    let theirs = s.event("them", r#"{"hook_event_name":"Stop"}"#);
    let parsed: serde_json::Value = serde_json::from_str(theirs.trim()).unwrap();
    assert_eq!(parsed["decision"], "block", "our own failure did not hold us: {theirs}");

    let said = text(&s.run_as("them", &["run", "--after", "1", "--", "sleep 2; exit 3"]));
    let id = said.split("detached as ").nth(1).unwrap().split('.').next().unwrap().trim().to_string();
    s.run_as("them", &["wait", &id]);
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

    for id in &ids {
        assert_eq!(s.run_with(&cap, &["wait", id]).status.code(), Some(0));
    }
    // AND A DELIBERATE JOB IS ANNOUNCED WHATEVER ITS DURATION. Somebody
    // chose to hand it over; a two-second one they chose to hand over is
    // still an ending they are waiting for.
    let told = text(&s.run_as("me", &["signals", "agent"]));
    for id in &ids {
        assert!(told.contains(id), "{id} was never announced:\n{told}");
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
    let said = text(&s.run_with(&quiet, &["run", "--after", "1", "--", "sleep 6"]));
    let id = said.split("detached as ").nth(1).unwrap().split('.').next().unwrap().trim().to_string();
    std::thread::sleep(std::time::Duration::from_secs(2));

    let out = s.run_with(&quiet, &["health"]);
    let shown = text(&out);
    // RUNNING IS NOT MAKING PROGRESS, and the two look identical from
    // outside. Freshness of the log is what separates them, and a job
    // that says nothing is NAMED rather than counted — a number here
    // would send the reader to `list` to find out which one.
    assert!(shown.contains(&id), "the mute job was not named:\n{shown}");
    assert!(shown.contains("MUTE"), "muteness was not said:\n{shown}");
    assert_eq!(out.status.code(), Some(1), "health said all is well");
    s.run(&["kill", &id]);
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
        .arg(format!("{JBX} config | head -1"))
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
    let id = said.split("detached as ").nth(1).unwrap().split('.').next().unwrap().trim().to_string();

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
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md")).unwrap();
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
    assert!(said.contains("detached as j"), "not detached at all: {said}");
    assert!(
        !said.contains("reading its standard input"),
        "an ordinary pipeline was called stuck:\n{said}"
    );
    // AND NOTHING IN IT PREDICTS. The costly half of the old message was
    // not the guess but the certainty: "it will not finish on its own",
    // about a deployment that had.
    assert!(!said.contains("will not finish"), "it still predicts:\n{said}");

    let id = said.split("detached as ").nth(1).unwrap().split('.').next().unwrap().trim().to_string();
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

    let shown = text(&s.run(&["stats"]));
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

    let shown = text(&s.run(&["stats"]));
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
    let shown = text(&s.run(&["stats", "--project-path"]));
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
        .arg(format!("{JBX} run -- 'seq 20000' | head -3"))
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
        &format!("{JBX} run --after 3 -- 'sleep 6; echo REAL'"),
    ]));
    let id = said.split("detached as ").nth(1).unwrap().split('.').next().unwrap().trim().to_string();

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
    let roots = s.0.join("jobs/sessions");
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
    assert_eq!(filed, here.display().to_string(), "it did not fall back to the cwd");
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
}

#[test]
fn queue_says_out_loud_when_a_job_does_not_start() {
    let s = Scratch::new("stacked");
    let one = [("JBX_SLOTS", "1")];
    let first = text(&s.run_with(&one, &["queue", "a", "--", "sleep 3"]));
    let second = text(&s.run_with(&one, &["queue", "b", "--", "sleep 3"]));

    // A VERB THAT ANSWERS WITH AN ID AND NOTHING ELSE lets somebody
    // believe the work has begun. A job held back by a full queue looks
    // exactly like one already running, until they go and look.
    assert!(second.contains("NOT STARTED"), "the second one kept quiet:\n{second}");
    assert!(!first.contains("NOT STARTED"), "the first one claimed to be held:\n{first}");
    // AND THE ID IS STILL THE FIRST LINE, ALONE, because that is what a
    // script reads; the rest is for a person.
    assert!(first.lines().next().unwrap().trim().starts_with('j'));
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
    let tickets = s.0.join("jobs/slots/tickets");
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
    let line = "cd /tmp && echo a very long line indeed that a column would cut; sleep 4";
    s.run(&["run", "--after", "1", "--", line]);
    std::thread::sleep(std::time::Duration::from_millis(1400));

    // BOTH COLUMNS WHEN THERE ARE TWO THINGS TO SAY. Nobody named this
    // one, so its intent would be the first four words of the line
    // printed beside the line — a column that repeats its neighbour. It
    // appears when a caller actually said something, and not before.
    let short = text(&s.run(&["ps", "--width", "60"]));
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
        "run", "--after", "1", "--intent", "replay the DAG simulation", "--", "sleep 3",
    ]));
    let id = said.split("detached as ").nth(1).unwrap().split('.').next().unwrap().trim().to_string();
    let listed = text(&s.run(&["ps", "--width", "200"]));
    assert!(listed.contains("replay the DAG simulation"), "the name was dropped:\n{listed}");
    // AND IT IS CUT TO THE COLUMN, not to a number written years ago.
    let narrow = text(&s.run(&["ps", "--width", "80"]));
    assert!(!narrow.contains("replay the DAG simulation"), "80 columns drew 200:\n{narrow}");
    assert!(narrow.contains("replay the"), "the name went missing entirely:\n{narrow}");

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
    let _ = s.run(&["wait", &id]);
}
