//! WHAT EACH AGENT CLI CALLS A SHELL LINE, AND WHERE IT WANTS THE ANSWER.
//!
//! Six programs run commands on a model's behalf, and all six offer the
//! same favour: a hook that sees a tool call before it happens and may
//! rewrite it. They do not agree on a single word of how.
//!
//! MEASURED, NOT READ OFF A PAGE. Each shape below was found by sending
//! payloads at a client's own hook processor and reading what came back
//! — which is how two of them turned out to key on a tool name other
//! than `Bash`, after first appearing to be broken.
//!
//! IT IS A TABLE ON PURPOSE. A branch per client is six places to forget
//! something; a row per client is a row. `jbx describe` publishes it, so
//! a test can check that every dialect named here actually answers, and
//! the guard that once caught `jbx after` shipped-but-undeclared covers
//! these too.
//!
//! WHAT IS NOT HERE IS THE HARD PART, and it turned out not to be hard.
//! jbx used to need four events: one to wrap, three to carry an ending
//! back. But `jbx wait <id>` is an ordinary command that exits when the
//! job does — run in the background by whatever runs the caller's
//! commands, it delivers the ending through no hook at all, and the
//! detachment message says so on the spot. So a client needs exactly one
//! hook to be worth supporting, and that is why this table is short
//! rather than a project.

use serde_json::{Map, Value};

/// How one client speaks.
pub struct Dialect {
    /// What to type: `jbx hook <name>`.
    pub name: &'static str,
    /// What this client calls the tool that runs a shell line. Three
    /// spellings across six clients, and a mismatch here looks exactly
    /// like a hook that never fired.
    pub tool: &'static str,
    /// Where the rewritten input object belongs in the answer.
    pub at: &'static [&'static str],
    /// WHAT THE CLIENT CALLS THE TWO FIELDS IT SENDS US. Most say
    /// `tool_name` and `tool_input`; Copilot's own format says `toolName`
    /// and `toolArgs`. Reading the wrong key finds nothing, and finding
    /// nothing is indistinguishable from a tool we were not watching.
    pub reads_tool: &'static str,
    pub reads_input: &'static str,
    /// WHETHER THE CLIENT REPLACES ITS TOOL INPUT WITH WHAT WE SEND, or
    /// merges it in. Claude replaces the whole object, so a field we
    /// leave out is a field we deleted — `timeout` above all, which is
    /// the caller saying how long they were prepared to wait. Gemini
    /// merges, so only the changed key need travel.
    ///
    /// Sending everything is safe under both, and this flag exists to
    /// say which behaviour is being relied on rather than to save bytes.
    pub replaces: bool,
    /// The event name this client uses for "before a tool runs", for
    /// declaring the hook and for recognising the payload.
    pub before_tool: &'static str,
    /// Where a human-readable note about the rewrite goes, if the client
    /// has a place for one. A NOTE, NOT A DECISION: it says why the line
    /// changed, and grants nothing.
    pub note_key: Option<&'static str>,

    // ── WHERE THE HOOK IS DECLARED ───────────────────────────────────
    /// The directory under the user's home where this client keeps its
    /// settings: `.claude`, `.gemini`. **Empty when `jbx init` cannot
    /// declare for this client yet** — three of them keep their hooks in
    /// a file of another name and another shape (`~/.copilot/hooks/*.json`
    /// with a flat list, `~/.cursor/hooks.json`, `~/.factory/hooks.json`
    /// with the events at the root), and writing Claude's shape there
    /// would produce a hook that never fires and looks installed. `jbx
    /// hook <name>` answers them all the same; only declaring is missing.
    ///
    /// Where it IS set, the file inside is `settings.json` and the entry
    /// has the same shape — a matcher with a list of `{type, command}` —
    /// read in each client's own reference rather than assumed from one.
    pub home_dir: &'static str,
    /// An environment variable that moves that directory, where the
    /// client offers one. Empty when it does not — Gemini's reference
    /// documents project, user and system files and no override.
    pub config_env: &'static str,

    // ── THE EVENTS THAT SPEAK WITHOUT BEING ASKED ────────────────────
    //
    // None is needed to detach a line; `jbx wait` carries an ending
    // through no hook at all. They are what `jbx init --announce` adds,
    // and a client with no equivalent gets `None` — so the flag says it
    // has nothing to declare rather than declaring names that client has
    // never heard of.
    /// The turn is ending. Claude `Stop`, Gemini `AfterAgent`.
    pub turn_end: Option<&'static str>,
    /// A turn is beginning. Claude `UserPromptSubmit`, Gemini `BeforeAgent`.
    pub turn_start: Option<&'static str>,
    /// The session is beginning — where the discipline is said once.
    pub session_start: Option<&'static str>,
    /// THE WORD THAT HOLDS A SESSION OPEN, and it is not the same word.
    /// Claude wants `decision: "block"`; Gemini's `AfterAgent` wants
    /// `"deny"`, with the `reason` sent back to the agent as a fresh
    /// prompt. Same effect, different spelling — and the wrong one is an
    /// unknown value, not an error anybody would ever see.
    pub hold: &'static str,
}

/// EVERY CLIENT WHOSE CONTRACT HAS BEEN READ.
///
/// THE STANDARD IS THE ONE ACTUALLY APPLIED, and it used to be written
/// stricter than it was kept: "exercised, not documentation read". No
/// real Gemini was ever exercised — its OFFICIAL REFERENCE was read and
/// checked against a probed shape, and that disagreement is what caught
/// a decision value the reference does not contain. So the rule is:
///
///   **the client's own reference, agreeing with a probed shape.**
///
/// Neither documentation alone nor another tool's belief about it.
///
/// THAT DISTINCTION EARNED ITS KEEP. A widely used proxy watches for
/// `Bash` on Cursor, Droid and Copilot; their references say `Shell`,
/// `Execute` and `bash`. Copying it would have installed three hooks
/// that never fire — and a hook that never fires is silent, not wrong,
/// which is the failure this table exists to make impossible.
///
/// TWO CLIENTS ARE DELIBERATELY ABSENT, and the reason is recorded so
/// nobody re-investigates from nothing:
///
/// - **Mistral Vibe** — hooks shipped experimental in v2.15.0 and
///   changed again by v2.21.0, declared in `hooks.toml`. The shape is
///   moving; the reference has not been read in full.
/// - **Meta Muse Code** — no source states whether its `PreToolUse` can
///   REWRITE at all, as opposed to allow/deny/ask. Its hooks live behind
///   an experimental plugin flag, and its documented `.muse/hooks.json`
///   is reported to be silently ignored by the binary. Worse, unknown
///   output keys are said to FAIL the hook — so guessing `updatedInput`
///   would break it rather than be ignored. One question decides it:
///   can it rewrite?
pub const DIALECTS: &[Dialect] = &[
    Dialect {
        name: "claude",
        tool: "Bash",
        at: &["hookSpecificOutput", "updatedInput"],
        reads_tool: "tool_name",
        reads_input: "tool_input",
        replaces: true,
        before_tool: "PreToolUse",
        note_key: Some("permissionDecisionReason"),
        home_dir: ".claude",
        config_env: "CLAUDE_CONFIG_DIR",
        turn_end: Some("Stop"),
        turn_start: Some("UserPromptSubmit"),
        session_start: Some("SessionStart"),
        hold: "block",
    },
    Dialect {
        name: "gemini",
        // NOT `Bash`. Gemini calls it `run_shell_command`, and a hook
        // watching for the wrong name answers nothing while looking
        // perfectly healthy — which is how this was nearly missed.
        tool: "run_shell_command",
        at: &["hookSpecificOutput", "tool_input"],
        reads_tool: "tool_name",
        reads_input: "tool_input",
        // Its own reference says the object "merges with and overrides
        // the model's arguments", so a field omitted is a field kept.
        replaces: false,
        before_tool: "BeforeTool",
        // Its `systemMessage` is shown to the PERSON, not the model, and
        // a line of ours on their screen for every command they run is
        // noise they did not ask for. Left empty.
        note_key: None,
        home_dir: ".gemini",
        config_env: "",
        turn_end: Some("AfterAgent"),
        turn_start: Some("BeforeAgent"),
        session_start: Some("SessionStart"),
        hold: "deny",
    },
    Dialect {
        // FACTORY DROID speaks Claude's shape exactly — same envelope,
        // same event names — and calls its shell tool `Execute`. rtk
        // watches for `Bash` here, which is a hook that never fires.
        name: "droid",
        tool: "Execute",
        at: &["hookSpecificOutput", "updatedInput"],
        reads_tool: "tool_name",
        reads_input: "tool_input",
        replaces: true,
        before_tool: "PreToolUse",
        note_key: Some("permissionDecisionReason"),
        // `~/.factory/hooks.json`, with the events at the ROOT rather
        // than under a `hooks` key — close to Claude's shape but not it,
        // so declaring is left out rather than done wrong.
        home_dir: "",
        config_env: "",
        turn_end: Some("Stop"),
        turn_start: Some("UserPromptSubmit"),
        session_start: Some("SessionStart"),
        hold: "block",
    },
    Dialect {
        // CURSOR rewrites through `preToolUse`, not `beforeShellExecution`
        // — that one's own reference says the output "does not support
        // modifying the command itself". And its shell tool is `Shell`.
        name: "cursor",
        tool: "Shell",
        at: &["updated_input"],
        reads_tool: "tool_name",
        reads_input: "tool_input",
        replaces: true,
        before_tool: "preToolUse",
        // Its notes are `user_message` (shown) and `agent_message` (sent
        // when denied); neither is a place for a line about a rewrite we
        // are not asking permission for.
        note_key: None,
        home_dir: "",
        config_env: "",
        // `stop` and `beforeSubmitPrompt` exist, but their contracts have
        // not been read; `None` says unmeasured, not absent.
        turn_end: None,
        turn_start: None,
        session_start: None,
        hold: "deny",
    },
    Dialect {
        // COPILOT'S OWN FORMAT, not the Claude-compatible one rtk emits.
        // It sends `toolName`/`toolArgs`, calls the shell `bash`, and
        // takes the rewrite as a TOP-LEVEL `modifiedArgs`.
        //
        // Rewriting was ignored until github/copilot-cli#2013 was fixed;
        // a maintainer confirmed `updatedInput`/`modifiedArgs` are now
        // respected. So this needs a recent Copilot, and an older one
        // runs the original line rather than failing — which is the
        // quiet way for this to be wrong.
        name: "copilot",
        tool: "bash",
        at: &["modifiedArgs"],
        reads_tool: "toolName",
        reads_input: "toolArgs",
        replaces: true,
        before_tool: "preToolUse",
        note_key: None,
        // `~/.copilot/hooks/*.json`: a `version` and a flat list with
        // `bash`/`powershell` keys instead of `command`. Another shape
        // entirely.
        home_dir: "",
        config_env: "",
        turn_end: Some("agentStop"),
        turn_start: Some("userPromptSubmitted"),
        session_start: Some("sessionStart"),
        hold: "deny",
    },
];

/// The dialect with this name, if it is one we speak.
///
/// Recognising which harness we are INSIDE is a different question, and
/// it lives in `harness.rs` — one place, so a new client is one row.
pub fn of(name: &str) -> Option<&'static Dialect> {
    DIALECTS.iter().find(|d| d.name == name)
}

impl Dialect {
    /// Build the answer this client expects around a rewritten input.
    ///
    /// NO `decision`, NO `permissionDecision`, IN EITHER DIALECT — and
    /// for the same reason in both. Claude's `allow` and Gemini's
    /// omission-means-allow would each let this hook grant a command it
    /// cannot judge; it wraps EVERY line, so answering for the person
    /// would answer for the one they meant to refuse. Gemini's contract
    /// helps here: allowing is what omitting does, so saying nothing is
    /// both the safe answer and the neutral one.
    ///
    /// (rtk emits `"decision":"ask_user"` for Gemini. That value is not
    /// in the reference, which lists only `deny`/`block`; we leave it
    /// out rather than copy it.)
    pub fn answer(&self, input: Map<String, Value>) -> Value {
        let mut out = Value::Object(Map::new());
        let mut cursor = &mut out;
        for (i, key) in self.at.iter().enumerate() {
            let last = i + 1 == self.at.len();
            let next = if last {
                Value::Object(input.clone())
            } else {
                Value::Object(Map::new())
            };
            cursor[*key] = next;
            cursor = &mut cursor[*key];
        }
        // THE EVENT NAME IS ECHOED WHERE THE CLIENT LOOKS FOR IT. Claude
        // reads `hookSpecificOutput.hookEventName` and ignores the block
        // without it; Gemini does not ask, and an extra key is harmless.
        if self.at.first() == Some(&"hookSpecificOutput") {
            out["hookSpecificOutput"]["hookEventName"] = Value::String(self.before_tool.into());
        }
        if let Some(key) = self.note_key {
            let parent = &self.at[..self.at.len() - 1];
            let mut at = &mut out;
            for k in parent {
                at = &mut at[*k];
            }
            at[key] = Value::String("jbx: wrapped so a long line can be detached".into());
        }
        out
    }
}
