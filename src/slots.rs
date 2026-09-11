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

/// HOW WIDE THE QUEUE OPENS.
///
/// Half the cores, which is the default the queue this replaces used and
/// for the same reason: most queued work is I/O-bound and half the cores
/// throttles it for little, but the caller here is usually an agent, and
/// an unbounded queue driven by one does not survive a loop that files
/// fifty jobs. `JBX_SLOTS=none` is one word away.
/// THE CAP COMES FROM THE CONFIGURATION AND FROM NOWHERE ELSE.
///
/// `jbx slots <n>` used to write its own file, which sat between the
/// environment and the config and was invisible to both. So `jbx slots`
/// answered 3 while `jbx config` — whose entire job is to say every
/// value AND WHERE IT CAME FROM — answered 6, `default`. Measured, not
/// deduced. A state carrying the same name as a setting IS that setting,
/// copied, and the copy is what drifts.
///
/// The verb writes the setting now, so there is one place to read.
pub fn cap() -> Option<usize> {
    crate::config::slots(default_cap()).0
}

pub fn default_cap() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).max(1))
        .unwrap_or(2)
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
    let mut held: Vec<(PathBuf, u32)> = Vec::new();
    let Ok(entries) = fs::read_dir(slots_dir()) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lock") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else { continue };
        let Ok(pid) = text.trim().parse::<u32>() else { continue };
        held.push((path, pid));
    }
    let live = store::alive_many(&held.iter().map(|(_, p)| *p).collect::<Vec<_>>());
    for (path, pid) in held {
        if !live.contains(&pid) {
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
    let waiting = outstanding();
    let pids: Vec<u32> = waiting.iter().map(|(_, pid)| *pid).collect();
    let live = store::alive_many(&pids);
    for (number, pid) in waiting {
        if !live.contains(&pid) {
            let _ = fs::remove_file(tickets_dir().join(format!("{number}.{pid}")));
        }
    }
}

/// Take the next number. ATOMIC, because `create_new` is: two waiters
/// racing for the same number cannot both have it, and the loser simply
/// tries the next one.
/// A PLACE IN LINE, OR NONE IF THERE IS NO LINE TO STAND IN.
///
/// A number somebody else holds moves us to the next one. Any other
/// failure means the directory is not there to write in — deleted with
/// `cache/`, or never creatable — and this used to retry that same
/// impossible write with no pause and no end: found at 94% CPU for
/// twenty minutes, on a supervisor whose store had been deleted under
/// it. The directory is made again once; after that we give up.
fn take_ticket() -> Option<Ticket> {
    let _ = fs::create_dir_all(tickets_dir());
    let mut candidate = outstanding().last().map(|(n, _)| n + 1).unwrap_or(1);
    let pid = std::process::id();
    let mut made_again = false;
    loop {
        let path = tickets_dir().join(format!("{candidate}.{pid}"));
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Some(Ticket { number: candidate, path: Some(path) }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => candidate += 1,
            Err(_) if !made_again => {
                made_again = true;
                let _ = fs::create_dir_all(tickets_dir());
            }
            Err(_) => return None,
        }
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
///
/// `None` WHEN THE WAIT IS OVER WITHOUT A SLOT: no ticket could be taken,
/// or `still_wanted` says the job is gone. Both used to mean waiting for
/// ever — a supervisor outliving its deleted `cache/` sat there idle, or
/// spinning, until somebody killed it.
pub fn wait_for_one(still_wanted: impl Fn() -> bool) -> Option<Held> {
    // NO CAP MEANS NO LINE. Taking a number to stand in a queue nobody
    // is holding would be ceremony, and one more file to clean up.
    if cap().is_none() {
        return Some(Held(None));
    }
    let ticket = take_ticket()?;
    loop {
        // ASKED FIRST, before anything below recreates a directory for a
        // job that nobody can find any more.
        if !still_wanted() {
            return None;
        }
        reclaim_dead_tickets();
        let head = outstanding().first().map(|(n, _)| *n);
        if head == Some(ticket.number) {
            if let Some(held) = try_take() {
                return Some(held); // the ticket is dropped with us, releasing our place
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
