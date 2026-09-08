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
}

/// EVERY CLIENT MEASURED SO FAR. Two are wired; the rest are recorded
/// because a shape found and then forgotten has to be found again.
///
/// A row is added when its client has been exercised, not when its
/// documentation has been read — an integration that silently fails to
/// wrap is worse than an absent one, since it promises the discipline
/// without keeping it.
pub const DIALECTS: &[Dialect] = &[
    Dialect {
        name: "claude",
        tool: "Bash",
        at: &["hookSpecificOutput", "updatedInput"],
        replaces: true,
        before_tool: "PreToolUse",
        note_key: Some("permissionDecisionReason"),
    },
    Dialect {
        name: "gemini",
        // NOT `Bash`. Gemini calls it `run_shell_command`, and a hook
        // watching for the wrong name answers nothing while looking
        // perfectly healthy — which is how this was nearly missed.
        tool: "run_shell_command",
        at: &["hookSpecificOutput", "tool_input"],
        // Its own reference says the object "merges with and overrides
        // the model's arguments", so a field omitted is a field kept.
        replaces: false,
        before_tool: "BeforeTool",
        // Its `systemMessage` is shown to the PERSON, not the model, and
        // a line of ours on their screen for every command they run is
        // noise they did not ask for. Left empty.
        note_key: None,
    },
];

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
