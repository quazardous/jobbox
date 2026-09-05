//! WIRING THE WRAPPER IN — AND TAKING rtk'S HOOK OUT.
//!
//! Two hooks that both rewrite `command` are two writers of one value,
//! in an order the harness does not document: whoever writes last erases
//! the other. So there is ONE hook here, and it calls rtk itself (see
//! `hook.rs`). Removing rtk's registration is not hostile to it — it is
//! what keeps its effect, because ours would otherwise erase it half the
//! time, silently, at random.
//!
//! WHAT IS TAKEN OUT IS WRITTEN DOWN BEFORE IT IS TAKEN. `undo` puts it
//! back exactly. Removing this wrapper must not leave a machine with
//! neither the wrapper nor rtk — that would be the one outcome worse
//! than doing nothing.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// The settings file the harness reads for every session.
///
/// The USER-LEVEL one, deliberately: it is where rtk registers, so it is
/// where the collision is. A project-level file cannot unregister a hook
/// declared for the whole account.
pub fn settings_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join("settings.json"));
    }
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    Some(PathBuf::from(home?).join(".claude").join("settings.json"))
}

fn saved_path() -> PathBuf {
    crate::store::dir().join("displaced-hooks.json")
}

fn read(path: &Path) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}))
}

/// WRITTEN WHOLE OR NOT AT ALL. This file is what makes the user's
/// sessions start; a half-written one is a broken harness, and it would
/// break at the next launch rather than here, where it could be
/// explained.
fn write(path: &Path, value: &Value) -> std::io::Result<()> {
    let tmp = path.with_extension("json.jbx-part");
    fs::write(&tmp, serde_json::to_string_pretty(value)? + "\n")?;
    fs::rename(&tmp, path)
}

fn is_rtk(entry: &Value) -> bool {
    entry["command"]
        .as_str()
        .map(|c| c.split_whitespace().next() == Some("rtk"))
        .unwrap_or(false)
}

fn is_ours(entry: &Value, binary: &str) -> bool {
    entry["command"]
        .as_str()
        .and_then(|c| c.split_whitespace().next())
        .map(|first| Path::new(first).file_name().and_then(|n| n.to_str()) == Path::new(binary).file_name().and_then(|n| n.to_str()))
        .unwrap_or(false)
}

/// Whether this binary is already declared for an event.
fn declared(settings: &Value, event: &str, binary: &str) -> bool {
    settings["hooks"][event]
        .as_array()
        .map(|matchers| {
            matchers.iter().any(|m| {
                m["hooks"]
                    .as_array()
                    .map(|hooks| hooks.iter().any(|e| is_ours(e, binary)))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// Add one entry, leaving everything already there alone.
///
/// A SETTINGS FILE BELONGS TO OTHER TOOLS TOO. Replacing the array would
/// be simpler and would delete somebody else's hook — the kind of edit
/// that is noticed a week later, by its absence.
fn declare(settings: &mut Value, event: &str, matcher: &str, binary: &str) {
    let entry = json!({
        "matcher": matcher,
        "hooks": [{ "type": "command", "command": format!("{binary} hook") }],
    });
    settings["hooks"][event] = match settings["hooks"][event].take() {
        Value::Array(mut a) => {
            a.push(entry);
            Value::Array(a)
        }
        _ => json!([entry]),
    };
}

/// Register our hook and displace rtk's.
pub fn init(undo: bool) -> i32 {
    let Some(path) = settings_path() else {
        eprintln!("jbx: cannot find the settings file — set CLAUDE_CONFIG_DIR");
        return 2;
    };
    // OUR OWN ABSOLUTE PATH. There is one binary, so the thing declared
    // to the harness is the thing running this — and by its path, never
    // its name: a hook runs with whatever PATH the harness has.
    let binary = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "jbx".into());
    if undo {
        return restore(&path, &binary);
    }

    // THE GLOBAL FILE IS WRITTEN WHEN THERE IS NONE, so the settings are
    // discoverable by reading rather than by asking. It is all comments
    // and one real key: nothing changes, and everything is named.
    let config = crate::config::path();
    if !config.exists() {
        if let Some(parent) = config.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::write(&config, crate::config::TEMPLATE).is_ok() {
            println!("wrote {}", config.display());
        }
    }

    // AND THE PROJECT'S OWN FILE, when this is run inside one. Written
    // fully commented, so it changes nothing — its job is to make the
    // settings findable by reading, and to record what rtk answered on
    // the day it was written rather than what somebody assumed later.
    //
    // ONLY WHERE A PROJECT ACTUALLY BEGINS. `project_root` falls back to
    // the working directory when it finds no marker, and dropping a
    // `.jbx.yaml` into whatever directory somebody happened to be in is
    // litter, not configuration.
    let root = crate::config::project_root();
    if root.join(".claude").exists() || root.join(".git").exists() {
        let local = root.join(".jbx.yaml");
        if !local.exists() {
            let rtk = crate::config::rtk_answers();
            if fs::write(&local, crate::config::project_template(rtk)).is_ok() {
                println!("wrote {}", local.display());
                println!("  everything in it is commented — it changes nothing until you");
                println!("  uncomment something. rtk {} when it was written.",
                         if rtk { "answered" } else { "did not answer" });
            }
        }
    }

    let mut settings = read(&path);
    let matchers = settings
        .pointer_mut("/hooks/PreToolUse")
        .and_then(Value::as_array_mut);
    let mut displaced: Vec<Value> = Vec::new();
    let mut already = false;
    let keep_rtk_hook = crate::config::compose().0 != crate::config::Compose::Never;

    if let Some(matchers) = matchers {
        for matcher in matchers.iter_mut() {
            // The label is taken BEFORE the hooks are borrowed to be
            // edited: one value cannot be read and rewritten at once,
            // and the label is what says where to put the entry back.
            let label = matcher["matcher"].clone();
            let Some(hooks) = matcher["hooks"].as_array_mut() else { continue };
            for entry in hooks.iter() {
                // `compose: never` MEANS LEAVE rtk ALONE — including its
                // hook. Unregistering a tool we have also decided not to
                // call would remove it from the machine altogether, which
                // is not what "never compose" asks for.
                if is_rtk(entry) && keep_rtk_hook {
                    displaced.push(json!({
                        "matcher": label.clone(),
                        "hook": entry.clone(),
                    }));
                }
                if is_ours(entry, &binary) {
                    already = true;
                }
            }
            if keep_rtk_hook {
                hooks.retain(|entry| !is_rtk(entry));
            }
        }
        matchers.retain(|m| !m["hooks"].as_array().map(|h| h.is_empty()).unwrap_or(false));
    }

    if !already {
        declare(&mut settings, "PreToolUse", "Bash", &binary);
    }
    // THE OTHER THREE ARE WHAT MAKES A DETACHED JOB WORTH DETACHING.
    //
    // `PreToolUse` wraps the line; these carry its ENDING back. Without
    // them a finished job is something you have to remember to check,
    // and remembering is precisely what letting go of it was supposed to
    // buy. `Stop` is the only one whose output reaches the model, so it
    // is the one that can hold a session open on a failure; the other
    // two simply say what landed.
    for (event, matcher) in [("Stop", "*"), ("UserPromptSubmit", "*"), ("SessionStart", "*")] {
        if !declared(&settings, event, &binary) {
            declare(&mut settings, event, matcher, &binary);
        }
    }

    if !displaced.is_empty() {
        let _ = fs::create_dir_all(crate::store::dir());
        // APPENDED, NEVER OVERWRITTEN: running `init` twice must not
        // replace the real record of what was displaced with an empty
        // one, which is exactly how an `undo` comes to restore nothing.
        let mut kept = read(&saved_path());
        let list = kept["displaced"].as_array().cloned().unwrap_or_default();
        let mut list = list;
        list.extend(displaced.clone());
        kept["displaced"] = Value::Array(list);
        let _ = write(&saved_path(), &kept);
    }

    if let Err(e) = write(&path, &settings) {
        eprintln!("jbx: cannot write {}: {e}", path.display());
        return 1;
    }
    outln!("wired into {}", path.display());
    if already {
        outln!("  (the hook was already declared — left as it was)");
    }
    for d in &displaced {
        outln!(
            "  displaced: {} — jbx now calls it itself, so its effect is kept",
            d["hook"]["command"].as_str().unwrap_or("?")
        );
    }
    if !displaced.is_empty() {
        outln!("  put it back with `jbx init --undo`");
    }
    0
}

fn restore(path: &Path, binary: &str) -> i32 {
    let mut settings = read(path);
    // EVERY EVENT WE EVER DECLARED, not just the one we came for: an
    // undo that leaves three of four behind is worse than no undo, since
    // what stays points at a binary that may not be there any more.
    for event in ["PreToolUse", "Stop", "UserPromptSubmit", "SessionStart"] {
        let Some(matchers) = settings
            .pointer_mut(&format!("/hooks/{event}"))
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        for matcher in matchers.iter_mut() {
            if let Some(hooks) = matcher["hooks"].as_array_mut() {
                hooks.retain(|e| !is_ours(e, binary));
            }
        }
        matchers.retain(|m| !m["hooks"].as_array().map(|h| h.is_empty()).unwrap_or(false));
        // AN EVENT WE EMPTIED IS AN EVENT WE ADDED, so the key goes with
        // it. Leaving `"Stop": []` behind would be a trace of a tool that
        // is no longer installed — harmless to the harness, and exactly
        // the kind of residue that makes people distrust an uninstall.
        if matchers.is_empty() {
            if let Some(hooks) = settings["hooks"].as_object_mut() {
                hooks.remove(event);
            }
        }
    }

    let kept = read(&saved_path());
    let displaced = kept["displaced"].as_array().cloned().unwrap_or_default();
    for d in &displaced {
        let wanted = d["matcher"].clone();
        let entry = d["hook"].clone();
        let matchers = settings["hooks"]["PreToolUse"].as_array_mut();
        match matchers {
            Some(matchers) => {
                if let Some(m) = matchers.iter_mut().find(|m| m["matcher"] == wanted) {
                    let hooks = m["hooks"].as_array_mut();
                    if let Some(hooks) = hooks {
                        if !hooks.contains(&entry) {
                            hooks.push(entry);
                        }
                    }
                } else {
                    matchers.push(json!({"matcher": wanted, "hooks": [entry]}));
                }
            }
            None => {
                settings["hooks"]["PreToolUse"] = json!([{"matcher": wanted, "hooks": [entry]}]);
            }
        }
    }
    if let Err(e) = write(path, &settings) {
        eprintln!("jbx: cannot write {}: {e}", path.display());
        return 1;
    }
    let _ = fs::remove_file(saved_path());
    outln!("removed from {}", path.display());
    for d in &displaced {
        outln!("  restored: {}", d["hook"]["command"].as_str().unwrap_or("?"));
    }
    // THE PROJECT'S FILE IS LEFT WHERE IT IS, and said so. It may have
    // been edited, and it may be committed — deleting somebody's
    // configuration because they uninstalled the reader of it is the
    // kind of tidiness that loses work.
    let local = crate::config::project_root().join(".jbx.yaml");
    if local.exists() {
        outln!("  left {} alone — it is yours", local.display());
    }
    0
}
