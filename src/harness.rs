//! WHICH AGENT CLI WE ARE RUNNING INSIDE — asked in ONE place.
//!
//! Three things need to know, and each used to find out for itself: the
//! mailbox name derived from a session id, the announcement deciding
//! whether to name Monitor, and the hook deciding whether an agent's own
//! `jbx wait` may be refused. Three readings of one fact is how they
//! come to disagree — one gets a new client and the others do not.
//!
//! ══════════════════════════════════════════════════════════════════
//!  ADDING A CLIENT HAPPENS HERE, AND ONLY HERE.
//!
//!  Add a row to `KNOWN` and everything that depends on recognising a
//!  harness follows: the mailbox, the wording of the detachment, whether
//!  `allow_wait` has anything to refuse. Nothing else needs editing.
//!
//!  A ROW IS EARNED BY OBSERVATION. Run the client, print its
//!  environment, and put what you SAW here — never what its
//!  documentation implies or what seems likely by analogy. A guessed
//!  variable makes jbx advise a gesture that does not exist in the tool
//!  it believes it is talking to, and an agent told to use a facility it
//!  cannot find has been sent nowhere.
//!
//!  There is one row today because one has been watched.
//! ══════════════════════════════════════════════════════════════════

/// One agent CLI, as recognised from the inside.
pub struct Harness {
    /// The dialect it speaks — the same name `jbx hook <client>` takes,
    /// so the two tables can be joined.
    pub client: &'static str,
    /// The environment variable that proves we are inside it. Its value
    /// is the session, when it has one.
    pub sign: &'static str,
    /// What its mailbox names start with.
    pub tag: &'static str,
    /// What it calls the thing that runs a command in the background for
    /// the agent, if it has one. THE NAME IS CARRIED RATHER THAN A
    /// BOOLEAN because the announcement prints it: a client whose
    /// facility is called something else should be told about by that
    /// name, not by Claude's.
    pub backgrounder: Option<&'static str>,
}

pub const KNOWN: &[Harness] = &[Harness {
    client: "claude",
    sign: "CLAUDE_CODE_SESSION_ID",
    tag: "cc",
    backgrounder: Some("Monitor"),
}];

/// The value of a variable, when it is set to something.
///
/// An empty variable is how a caller says "not this" — and a test that
/// cannot unset one has no other way to say it.
fn said(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// WHICH HARNESS THIS PROCESS IS INSIDE, read off the environment.
///
/// Not the same question as `jbx hook <client>`, which is TOLD. This is
/// asked by a `jbx run` deep inside a wrapped command, with nobody left
/// to tell it.
pub fn here() -> Option<&'static Harness> {
    KNOWN.iter().find(|h| said(h.sign).is_some())
}

/// The session as the harness wrote it, untouched.
///
/// Two callers want it at two lengths — a mailbox name and a directory —
/// so the ENVIRONMENT is read once here and each renders it its own way.
pub fn session_raw() -> Option<String> {
    said(here()?.sign)
}

/// The session this harness gives us, trimmed to something nameable.
pub fn session() -> Option<String> {
    let h = here()?;
    let value: String = session_raw()?
        .chars()
        .take(8)
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    (!value.is_empty()).then(|| format!("{}-{value}", h.tag))
}

/// WHAT THIS CLIENT CALLS ITS BACKGROUNDER, looked up by dialect name.
///
/// Asked by the hook, which knows the client because it was named on the
/// command line rather than found in the environment. Same table, other
/// door.
///
/// `None` is the answer that matters: a client with nothing to put a
/// wait into must never have the agent's own `jbx wait` refused, because
/// there it is the only way an ending reaches anybody.
pub fn backgrounder_of(client: &str) -> Option<&'static str> {
    KNOWN.iter().find(|h| h.client == client)?.backgrounder
}
