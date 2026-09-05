//! THE `PreToolUse` HOOK, AND WHY IT CALLS rtk INSTEAD OF RACING IT.
//!
//! rtk registers its own `PreToolUse` hook on `Bash`. So would this one.
//! Two hooks that both rewrite the `command` field are two writers of a
//! single value, and the order the harness applies them in is not
//! documented. Whoever writes last erases the other: either the wrapper
//! vanishes, or rtk's saving does.
//!
//! WAITING OUR TURN WOULD NOT HELP. A hook cannot read another hook's
//! output, so finishing later would mean overwriting the very rewrite we
//! waited for. The way out is to stop competing and CALL IT — on the
//! original line, before wrapping — and to unregister its own hook so it
//! cannot fire alongside us.
//!
//! MEASURED, on rtk 0.47.0: it rewrites only the FIRST WORD of a line,
//! and on a first word it does not know it prints NOTHING — not an empty
//! object, nothing. So it never touches a line already wrapped, and
//! calling it twice costs nothing.

use std::io::Read;
use std::process::{Command, Stdio};

use serde_json::Value;

/// rtk's rewrite of `line`, or `line` unchanged.
///
/// IT IS NEVER ALLOWED TO BREAK A COMMAND. Absent, slow, crashed, or
/// answering in a shape we do not recognise: the original line comes
/// back. The worst case is a lost saving, never a lost command.
pub fn through_rtk(line: &str) -> String {
    use crate::config::Compose;
    let (mode, _) = crate::config::compose();
    if mode == Compose::Never {
        return line.to_string();
    }
    let event = serde_json::json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": line, "description": "", "timeout": 0 },
    })
    .to_string();

    let child = Command::new("rtk")
        .args(["hook", "claude"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        // `always` MEANS SOMEBODY IS COUNTING ON IT. Failing quietly is
        // right for `auto` and wrong here: a broken install would
        // otherwise look exactly like a machine that never had rtk.
        if mode == Compose::Always {
            eprintln!("jbx: rtk is declared `always` but could not be run — \
                       the line is wrapped as it is");
        }
        return line.to_string();
    };
    if let Some(mut sink) = child.stdin.take() {
        use std::io::Write;
        let _ = sink.write_all(event.as_bytes());
    }
    let Ok(out) = child.wait_with_output() else { return line.to_string() };
    let text = String::from_utf8_lossy(&out.stdout);
    if text.trim().is_empty() {
        return line.to_string(); // it does not know this command
    }
    let parsed: Result<Value, _> = serde_json::from_str(text.trim());
    match parsed {
        Ok(v) => v["hookSpecificOutput"]["updatedInput"]["command"]
            .as_str()
            .filter(|c| !c.trim().is_empty())
            .unwrap_or(line)
            .to_string(),
        Err(_) => line.to_string(),
    }
}

/// A line as one shell word, so the wrapper receives it whole.
///
/// THE LINE IS WRAPPED WHOLE, NEVER COMMAND BY COMMAND, and this is what
/// makes that true. `make && deploy` wrapped in two pieces would give
/// `make` a detachment code of 0 and send `deploy` against a tree that
/// was never built. Wrapped whole, there is one exit code because there
/// is one command — the shell's.
pub fn quote(line: &str) -> String {
    format!("'{}'", line.replace('\'', r"'\''"))
}

/// A path as one shell word, quoted only if it has to be.
///
/// Quoting unconditionally would be simpler and would break the reading
/// above: a line starting with a quote no longer starts with our name,
/// so we would stop recognising our own output.
pub fn shell_word(path: &str) -> String {
    if path.bytes().all(|b| b.is_ascii_alphanumeric() || b"/._-+=:".contains(&b)) {
        path.to_string()
    } else {
        quote(path)
    }
}

/// Whether a line is one this wrapper already wrote.
///
/// The FILE NAME is what identifies us — an absolute path, a relative
/// one, a symlink and a copy on the PATH are all the same tool, and a
/// string comparison would call three of them somebody else.
pub fn is_us(line: &str, binary: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or("");
    let first = first.trim_start_matches('\'').trim_end_matches('\'');
    let name = |p: &str| {
        std::path::Path::new(p)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    };
    match (name(first), name(binary)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Read the harness's event, and answer with the rewritten input.
///
/// SILENCE IS THE DEFAULT, and it is what rtk gets right: no output at
/// all when there is nothing to say. A hook that speaks on every call is
/// a hook that gets deleted, and one that errors takes the command down
/// with it.
pub fn hook(binary: &str) -> i32 {
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return 0;
    }
    let Ok(event) = serde_json::from_str::<Value>(&raw) else { return 0 };
    // TURNED OFF HERE MEANS TURNED OFF ENTIRELY. `enabled: false` in a
    // project's `.jbx.yaml` makes this hook say nothing at all — not a
    // longer threshold, not a quieter mode: commands run exactly as they
    // would with jbx uninstalled, which is the only promise worth making
    // to somebody who asked it to stay out of the way.
    if !crate::config::enabled().0 {
        return 0;
    }
    // ONE COMMAND, DISPATCHING ON THE EVENT THE HARNESS DECLARES.
    //
    // Four registrations that all read `jbx hook` beat four verbs a
    // person has to spell right in a settings file. The harness already
    // says which event it is; asking the reader to say it again is one
    // more place for the two to disagree.
    match event["hook_event_name"].as_str() {
        Some("PreToolUse") => {}
        Some("Stop") => return crate::signals::announce_stop(),
        // THE SESSION'S FIRST HOOK CARRIES THE RULE; every later one
        // carries only what has finished. Saying the rule again each turn
        // would make it wallpaper.
        Some("SessionStart") => {
            crate::signals::discipline();
            return crate::signals::announce_text();
        }
        Some("UserPromptSubmit") => return crate::signals::announce_text(),
        _ => return 0,
    }
    if event["tool_name"].as_str() != Some("Bash") {
        return 0;
    }
    let Some(tool_input) = event["tool_input"].as_object() else { return 0 };
    let Some(line) = tool_input.get("command").and_then(Value::as_str) else { return 0 };

    // IDEMPOTENCE, JUDGED ON THE FILE NAME AND NOT THE WHOLE PATH.
    //
    // Nothing guarantees a hook is never shown a line it has already
    // transformed — a harness that chains its hooks does it on the first
    // call — and wrapping a wrapper buries the real command one level
    // deeper every time. The line we wrote carries an ABSOLUTE path, so
    // comparing whole strings would miss it as soon as the binary is
    // reached by another name: what identifies us is the file name.
    if is_us(line, binary) {
        return 0;
    }

    // EVERY FIELD OF `tool_input` IS ECHOED BACK, not only the one we
    // changed: the harness replaces the whole object, so a field left
    // out is a field deleted — `timeout` above all, which is the caller
    // saying how long they were prepared to wait.
    let mut updated = tool_input.clone();
    // THE ABSOLUTE PATH, NOT THE BARE NAME. A hook runs with whatever
    // PATH the harness happens to have — which is not the one the person
    // who installed this had. A bare name here is a `command not found`
    // in place of every command on the machine, and the first test
    // written found exactly that.
    updated.insert(
        "command".into(),
        Value::String(format!("{} run -- {}", shell_word(binary), quote(&through_rtk(line)))),
    );
    // `permissionDecision` IS DELIBERATELY ABSENT. Setting it to "allow"
    // alongside an `updatedInput` makes the harness drop the rewrite
    // without a word (claude-code#15897) — the failure that leaves you
    // certain the hook never ran.
    let answer = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecisionReason": "jbx: wrapped so a long line can be detached",
            "updatedInput": Value::Object(updated),
        }
    });
    outln!("{answer}");
    0
}
