//! HOW MANY DELIBERATE JOBS MAY RUN AT ONCE.
//!
//! ────────────────────────────────────────────────────────────────────
//! WHY THIS DOES NOT APPLY TO WRAPPED LINES
//! ────────────────────────────────────────────────────────────────────
//!
//! A cap holds back work that has NOT STARTED. `jbx run` never holds
//! anything back: it wraps a command the caller was going to run either
//! way, and detaching it does not change how many processes exist. There
//! is nothing there to queue, so a cap on it would be a cap on nothing.
//!
//! `jbx queue` is the other door — work handed over BEFORE it starts, on
//! purpose, with a name. That work can wait, so that is where the cap
//! lives, and it is the same bargain the queue this replaces offered.
//!
//! ────────────────────────────────────────────────────────────────────
//! ONE LOCK FILE PER SLOT, AND WHY THAT IS ENOUGH
//! ────────────────────────────────────────────────────────────────────
//!
//! Creating a file that must not already exist is ATOMIC on both
//! platforms — `O_EXCL` on Unix, `CREATE_NEW` on Windows — so two
//! supervisors racing for the last slot cannot both win, with no daemon
//! and no lock server between them.
//!
//! A HOLDER THAT DIED KEEPS ITS SLOT UNTIL SOMEBODY NOTICES. So each
//! lock carries its pid, and a waiter that finds every slot taken checks
//! whether the holders are still there before waiting again. Without
//! that, one crash costs a slot until the machine reboots.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use crate::store;

fn slots_dir() -> PathBuf {
    store::dir().join("slots")
}

fn cap_file() -> PathBuf {
    slots_dir().join("cap")
}

/// HOW WIDE THE QUEUE OPENS.
///
/// Half the cores, which is the default the queue this replaces used and
/// for the same reason: most queued work is I/O-bound and half the cores
/// throttles it for little, but the caller here is usually an agent, and
/// an unbounded queue driven by one does not survive a loop that files
/// fifty jobs. `JBX_SLOTS=none` is one word away.
pub fn cap() -> Option<usize> {
    // `jbx slots <n>` WRITES A FILE, AND THAT FILE SITS BETWEEN THE
    // ENVIRONMENT AND THE CONFIGURATION: it is a deliberate gesture made
    // for this machine, so it beats what the config file decided once —
    // and loses to a variable typed for one run.
    if let Ok(text) = std::env::var("JBX_SLOTS") {
        if !text.is_empty() {
            return crate::config::slots(default_cap()).0;
        }
    }
    if let Ok(text) = fs::read_to_string(cap_file()) {
        return match text.trim() {
            "none" | "0" => None,
            other => other.parse().ok().or(Some(default_cap())),
        };
    }
    crate::config::slots(default_cap()).0
}

pub fn default_cap() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).max(1))
        .unwrap_or(2)
}

pub fn set_cap(value: &str) -> std::io::Result<()> {
    fs::create_dir_all(slots_dir())?;
    fs::write(cap_file(), value)
}

/// A slot held for as long as this value lives.
pub struct Held(Option<PathBuf>);

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
    }
}

/// Try to take one slot; `None` if they are all busy.
fn try_take() -> Option<Held> {
    let Some(cap) = cap() else {
        return Some(Held(None)); // no cap: everybody runs
    };
    let _ = fs::create_dir_all(slots_dir());
    for n in 0..cap {
        let path = slots_dir().join(format!("{n}.lock"));
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let _ = write!(file, "{}", std::process::id());
                return Some(Held(Some(path)));
            }
            Err(_) => continue,
        }
    }
    None
}

/// Drop the locks of holders that are no longer there.
///
/// Read the pid, ask the kernel, remove it if nobody answers. A slot
/// freed here is taken on the next turn of the loop, not here: removing
/// and claiming in one gesture would race two waiters into one slot.
fn reclaim_dead() {
    let Ok(entries) = fs::read_dir(slots_dir()) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lock") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let Ok(pid) = text.trim().parse::<u32>() else { continue };
        if !store::alive(pid) {
            let _ = fs::remove_file(&path);
        }
    }
}

/// A PLACE IN THE LINE, held for as long as this value lives.
///
/// Its number is what makes the queue a QUEUE. Without it, waiting was
/// "whoever asks first when a slot frees" — which in practice followed
/// the filing order, because waiters start asking in that order, and in
/// practice is not a guarantee: a waiter whose poll lands just after a
/// slot frees loses to one that polls just before, however long it has
/// been there.
pub struct Ticket {
    number: u64,
    path: Option<PathBuf>,
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn tickets_dir() -> PathBuf {
    slots_dir().join("tickets")
}

/// The number in a ticket's name, and the pid that holds it.
fn read_ticket(name: &str) -> Option<(u64, u32)> {
    let (number, pid) = name.split_once('.')?;
    Some((number.parse().ok()?, pid.parse().ok()?))
}

fn outstanding() -> Vec<(u64, u32)> {
    let Ok(entries) = fs::read_dir(tickets_dir()) else { return Vec::new() };
    let mut out: Vec<(u64, u32)> = entries
        .flatten()
        .filter_map(|e| read_ticket(&e.file_name().to_string_lossy()))
        .collect();
    out.sort_unstable();
    out
}

/// Drop the tickets of waiters that are no longer there.
///
/// A DEAD WAITER AT THE HEAD BLOCKS EVERYBODY BEHIND IT. That is the one
/// way a queue with an order can do worse than one without, so the check
/// is not an afterthought: it runs on every turn of the wait.
fn reclaim_dead_tickets() {
    for (number, pid) in outstanding() {
        if !store::alive(pid) {
            let _ = fs::remove_file(tickets_dir().join(format!("{number}.{pid}")));
        }
    }
}

/// Take the next number. ATOMIC, because `create_new` is: two waiters
/// racing for the same number cannot both have it, and the loser simply
/// tries the next one.
fn take_ticket() -> Ticket {
    let _ = fs::create_dir_all(tickets_dir());
    let mut candidate = outstanding().last().map(|(n, _)| n + 1).unwrap_or(1);
    let pid = std::process::id();
    loop {
        let path = tickets_dir().join(format!("{candidate}.{pid}"));
        if fs::OpenOptions::new().write(true).create_new(true).open(&path).is_ok() {
            return Ticket { number: candidate, path: Some(path) };
        }
        candidate += 1;
    }
}

/// WAIT FOR A SLOT, IN ORDER, however long it takes.
///
/// This blocks in the SUPERVISOR, never in the caller: `jbx queue`
/// returns an id straight away and the waiting happens behind it. A verb
/// that queued something and then stood there would be a foreground
/// command wearing a queue's name.
///
/// ONLY THE HEAD OF THE LINE MAY TAKE A SLOT. Checking that first is
/// what turns "whoever asks at the right moment" into a queue — and it
/// costs one directory listing per turn, on a path that is already
/// sleeping.
pub fn wait_for_one() -> Held {
    // NO CAP MEANS NO LINE. Taking a number to stand in a queue nobody
    // is holding would be ceremony, and one more file to clean up.
    if cap().is_none() {
        return Held(None);
    }
    let ticket = take_ticket();
    loop {
        reclaim_dead_tickets();
        let head = outstanding().first().map(|(n, _)| *n);
        if head == Some(ticket.number) {
            if let Some(held) = try_take() {
                return held; // the ticket is dropped with us, releasing our place
            }
        }
        reclaim_dead();
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// How many slots are held right now, and how many there are.
pub fn busy() -> (usize, Option<usize>) {
    reclaim_dead();
    let held = fs::read_dir(slots_dir())
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("lock"))
                .count()
        })
        .unwrap_or(0);
    (held, cap())
}
