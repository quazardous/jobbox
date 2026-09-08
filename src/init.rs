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

/// The settings file one client reads for every session.
///
/// The USER-LEVEL one, deliberately: it is where rtk registers, so it is
/// where the collision is. A project-level file cannot unregister a hook
/// declared for the whole account.
///
/// THE DIRECTORY IS THE CLIENT'S, the file name is not: `.claude` and
/// `.gemini` both hold a `settings.json`, and both take an entry of the
/// same shape — a matcher with a list of `{type, command}`. That was
/// read in each client's own reference rather than assumed from one.
pub fn settings_path_for(d: &crate::dialect::Dialect) -> Option<PathBuf> {
    // NO ADDRESS MEANS WE CANNOT DECLARE HERE, and saying so beats
    // writing Claude's shape into a file that wants another one — three
    // clients keep their hooks in a differently named file with a
    // different structure, and a hook declared wrong never fires while
    // looking installed.
    if d.home_dir.is_empty() {
        return None;
    }
    if !d.config_env.is_empty() {
        if let Some(dir) = std::env::var_os(d.config_env) {
            return Some(PathBuf::from(dir).join("settings.json"));
        }
    }
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    Some(PathBuf::from(home?).join(d.home_dir).join("settings.json"))
}

/// The same, for Claude — the caller that has no dialect to hand.
pub fn settings_path() -> Option<PathBuf> {
    settings_path_for(crate::dialect::of("claude")?)
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

/// THE PATH TO WRITE INTO THE HARNESS — and it prefers the symlink.
///
/// `current_exe()` follows symlinks, so a dev install declared the build
/// tree itself. That is right in one way — a rebuild is picked up with no
/// re-init — and wrong in another: the hook is then nailed to a
/// directory, and moving or deleting the tree breaks every session at
/// once. Measured the hard way.
///
/// Declared through the link instead, both hold: a rebuild still follows
/// (the link points at the tree), and reinstalling or switching to a
/// release binary moves the hook with it, because the address stays the
/// same and only its target changes.
///
/// ONLY WHEN IT REALLY IS A LINK, and always as an absolute path: a hook
/// runs with whatever working directory and PATH the harness has, so a
/// bare name or a relative one would be a hook that works here and
/// nowhere else.
fn declared_binary() -> String {
    let resolved = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "jbx".into());
    let Some(invoked) = invocation_path() else { return shell_safe(resolved) };
    match fs::symlink_metadata(&invoked) {
        Ok(meta) if meta.file_type().is_symlink() => shell_safe(invoked.display().to_string()),
        _ => shell_safe(resolved),
    }
}

/// THE DECLARATION IS READ BY A SHELL, NOT BY WINDOWS.
///
/// The harness does not exec a hook's command; it hands the line to
/// `bash`, where a backslash is an escape character and not a separator.
/// MEASURED, from an install that had just reported success: a hook
/// declared as `C:\Users\berli\AppData\Local\jbx\bin\jbx.exe hook`
/// arrived as
///
///   /usr/bin/bash: line 1: C:UsersberliAppDataLocaljbxbinjbx.exe:
///   command not found
///
/// on every prompt, in every session, until the file was edited by hand.
/// Windows accepts a forward slash everywhere it accepts a backslash, so
/// the path is spelled the one way both of them read alike. Quoting was
/// the other candidate and is worse: it would fix `bash` and break `cmd`,
/// which strips no quotes of its own.
///
/// An existing declaration in the old spelling is repaired by the next
/// `jbx init`, since `repoint` already brings a stale one up to date.
#[cfg(windows)]
fn shell_safe(path: String) -> String {
    path.replace('\\', "/")
}

#[cfg(not(windows))]
fn shell_safe(path: String) -> String {
    path
}

/// The path this process was invoked by, made absolute WITHOUT resolving
/// links — which is the whole point, since resolving is what we are
/// trying not to do.
fn invocation_path() -> Option<PathBuf> {
    let argv0 = PathBuf::from(std::env::args_os().next()?);
    if argv0.is_absolute() {
        return Some(argv0);
    }
    if argv0.components().count() > 1 {
        // Typed as a path, so it is relative to where we stand.
        return Some(std::env::current_dir().ok()?.join(argv0));
    }
    // A bare name: whichever PATH entry would have been found first.
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(&argv0))
        .find(|candidate| candidate.is_file())
}

/// Rewrite our own entries that name a different path, and say how many.
fn repoint(settings: &mut Value, binary: &str) -> usize {
    let wanted = format!("{binary} hook");
    let mut changed = 0;
    let Some(hooks) = settings["hooks"].as_object_mut() else { return 0 };
    for (_event, matchers) in hooks.iter_mut() {
        let Some(matchers) = matchers.as_array_mut() else { continue };
        for matcher in matchers.iter_mut() {
            let Some(entries) = matcher["hooks"].as_array_mut() else { continue };
            for entry in entries.iter_mut() {
                if is_ours(entry, binary) && entry["command"].as_str() != Some(wanted.as_str()) {
                    entry["command"] = Value::String(wanted.clone());
                    changed += 1;
                }
            }
        }
    }
    changed
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
fn declare(settings: &mut Value, event: &str, matcher: &str, binary: &str, client: &str) {
    // THE CLIENT IS NAMED, ALWAYS — `jbx hook claude` as much as
    // `jbx hook gemini`.
    //
    // It was left off, and the failure was the quiet kind this whole
    // table exists to avoid: a bare `jbx hook` answers as CLAUDE, so a
    // hook declared for Gemini fired on `BeforeTool`, found the wrong
    // event name, and returned 0. Installed, matching, silent, useless.
    // Naming it removes the default that can mismatch.
    let entry = json!({
        "matcher": matcher,
        "hooks": [{ "type": "command", "command": format!("{binary} hook {client}") }],
    });
    settings["hooks"][event] = match settings["hooks"][event].take() {
        Value::Array(mut a) => {
            a.push(entry);
            Value::Array(a)
        }
        _ => json!([entry]),
    };
}

/// Take one entry of OURS back out, leaving every other tool's alone.
///
/// The mirror of `declare`, and it has to be as careful: a settings file
/// is shared, so this removes entries that name OUR binary and nothing
/// else, then drops a matcher only once it holds nothing — the empty
/// husk is what makes a file look edited by something careless.
///
/// Answers whether anything was actually removed, so the caller can say
/// what happened rather than claim it.
fn withdraw(settings: &mut Value, event: &str, binary: &str) -> bool {
    let Some(matchers) = settings.pointer_mut(&format!("/hooks/{event}")).and_then(Value::as_array_mut)
    else {
        return false;
    };
    let mut gone = false;
    for matcher in matchers.iter_mut() {
        if let Some(hooks) = matcher["hooks"].as_array_mut() {
            let before = hooks.len();
            hooks.retain(|e| !is_ours(e, binary));
            gone |= hooks.len() != before;
        }
    }
    matchers.retain(|m| !m["hooks"].as_array().map(|h| h.is_empty()).unwrap_or(true));
    if matchers.is_empty() {
        // AND THE EVENT ITSELF GOES when it was only ever ours. An
        // `"Stop": []` left behind is not wrong, but it says jbx is
        // still here to anyone reading the file.
        if let Some(hooks) = settings["hooks"].as_object_mut() {
            hooks.remove(event);
        }
    }
    gone
}

/// Register our hook and displace rtk's.
/// `announce` adds the three hooks that speak WITHOUT BEING ASKED;
/// `core` takes them back off. Neither is needed to detach a line — see
/// the note beside the declaration below.
pub fn init(
    undo: bool,
    global_only: bool,
    core: bool,
    announce: bool,
    d: &crate::dialect::Dialect,
) -> i32 {
    if d.home_dir.is_empty() {
        eprintln!("jbx: `init` cannot declare for {} yet — it keeps its hooks in a", d.name);
        eprintln!("  file of another name and another shape. `jbx hook {}` answers it;", d.name);
        eprintln!("  declaring is what is missing. Declare it by hand, calling:");
        eprintln!("    jbx hook {}", d.name);
        return 1;
    }
    let Some(path) = settings_path_for(d) else {
        eprintln!("jbx: cannot find the settings file — set CLAUDE_CONFIG_DIR");
        return 2;
    };
    let binary = declared_binary();
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
            outln!("wrote {}", config.display());
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
    // AND NOT AT ALL WHEN THE CALLER SAID SO. An installer runs from
    // wherever somebody happened to be standing, which may well be
    // inside a repository — and a file appearing in your project because
    // you installed a tool is a surprise, however commented it is.
    let root = crate::config::project_root();
    if !global_only && (root.join(".claude").exists() || root.join(".git").exists()) {
        let local = root.join(".jbx.yaml");
        if !local.exists() {
            let rtk = crate::config::rtk_answers();
            if fs::write(&local, crate::config::project_template(rtk)).is_ok() {
                outln!("wrote {}", local.display());
                outln!("  everything in it is commented — it changes nothing until you");
                outln!("  uncomment something. rtk {} when it was written.",
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

    // AN EXISTING DECLARATION IS BROUGHT UP TO DATE, not just noticed.
    // Re-running `init` after a reinstall used to leave the harness
    // pointing at the old path and say "already declared" — which reads
    // like "nothing to do" and was not.
    let moved = repoint(&mut settings, &binary);
    if !already {
        declare(&mut settings, d.before_tool, d.tool, &binary, d.name);
    }
    // THE OTHER THREE ARE NO LONGER PART OF THE DEAL, and measuring is
    // what moved them.
    //
    // They were declared because a detached job has to say when it ends,
    // and these were how it said so. But `jbx wait <id>` is an ORDINARY
    // COMMAND that exits when the job does — run in the background by
    // whatever runs the caller's commands, it delivers the same ending
    // through no hook of ours at all. That is the path actually used in
    // practice, and the detachment message names it on the spot.
    //
    // WHAT IS LOST WITHOUT THEM IS THE UNASKED ANNOUNCEMENT: a job
    // nobody armed a wait for finishes quietly, and stays quiet until
    // somebody runs `jbx ps`. Not lost — `jbx health` lists it — but not
    // volunteered either. That is a real difference, and it is a choice
    // rather than a default now, because three hooks in somebody else's
    // settings file is a large footprint for a case the message already
    // tells them how to cover.
    // FROM THE DIALECT, AND SOME CLIENTS HAVE NONE. Claude calls them
    // Stop / UserPromptSubmit / SessionStart, Gemini AfterAgent /
    // BeforeAgent / SessionStart. Declaring one client's names in
    // another's settings would be a hook that never fires and looks
    // installed.
    let unasked: Vec<&str> = [d.turn_end, d.turn_start, d.session_start]
        .into_iter()
        .flatten()
        .collect();
    let mut added = Vec::new();
    let mut taken = Vec::new();
    if announce {
        for event in &unasked {
            if !declared(&settings, event, &binary) {
                declare(&mut settings, event, "*", &binary, d.name);
                added.push(*event);
            }
        }
        if unasked.is_empty() {
            outln!("  ({} reports no event for an unasked ending — nothing to add)", d.name);
        }
    } else if core {
        // `--core` REPAIRS, it does not merely abstain. Somebody asking
        // for the small install on a machine that already has the large
        // one means "take the rest back out", and a flag that left them
        // in would be a flag that did nothing on the only machines where
        // it was worth typing.
        for event in &unasked {
            if withdraw(&mut settings, event, &binary) {
                taken.push(*event);
            }
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
    if fs::symlink_metadata(&binary).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        // SAID, BECAUSE IT CHANGES WHAT A REBUILD DOES. Through a link
        // the hook follows whatever the link points at — which is what
        // somebody working on a checkout wants, and a surprise to
        // somebody who thought they had pinned one binary.
        outln!("  declared through {binary} — a link, so the hook");
        outln!("  follows whatever it points at.");
    }
    // WHAT THE FLAGS DID, SAID OUT LOUD. A flag whose whole effect is
    // three lines in a file nobody opens is a flag you cannot tell you
    // ran, and `--core` in particular is worth typing precisely on the
    // machines where it has something to remove.
    if !added.is_empty() {
        outln!("  also declared {} — endings will now be reported unasked", added.join(", "));
    }
    if !taken.is_empty() {
        outln!("  took back {} — endings now come from `jbx wait`, not a hook", taken.join(", "));
    } else if core {
        outln!("  (only the wrapping hook was there — nothing to take back)");
    }
    if !announce && !core {
        outln!("  wrapping only. `--announce` adds the hooks that report an ending unasked.");
    }
    if moved > 0 {
        outln!("  {moved} declarations repointed at this binary");
    } else if already {
        outln!("  (the hook was already declared, and already correct)");
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
