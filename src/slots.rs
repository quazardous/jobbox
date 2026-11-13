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
use std::path::{Path, PathBuf};
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

/// Try to take one slot in `dir`; `None` if they are all busy.
fn try_take(dir: &Path, cap: Option<usize>) -> Option<Held> {
    let Some(cap) = cap else {
        return Some(Held(None)); // no cap: everybody runs
    };
    let _ = fs::create_dir_all(dir);
    for n in 0..cap {
        let path = dir.join(format!("{n}.lock"));
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
fn reclaim_dead(dir: &Path) {
    let mut held: Vec<(PathBuf, u32)> = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return };
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

fn tickets_dir(dir: &Path) -> PathBuf {
    dir.join("tickets")
}

/// The number in a ticket's name, and the pid that holds it.
fn read_ticket(name: &str) -> Option<(u64, u32)> {
    let (number, pid) = name.split_once('.')?;
    Some((number.parse().ok()?, pid.parse().ok()?))
}

fn outstanding(dir: &Path) -> Vec<(u64, u32)> {
    let Ok(entries) = fs::read_dir(tickets_dir(dir)) else { return Vec::new() };
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
fn reclaim_dead_tickets(dir: &Path) {
    let waiting = outstanding(dir);
    let pids: Vec<u32> = waiting.iter().map(|(_, pid)| *pid).collect();
    let live = store::alive_many(&pids);
    for (number, pid) in waiting {
        if !live.contains(&pid) {
            let _ = fs::remove_file(tickets_dir(dir).join(format!("{number}.{pid}")));
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
fn take_ticket(dir: &Path) -> Option<Ticket> {
    let _ = fs::create_dir_all(tickets_dir(dir));
    let mut candidate = outstanding(dir).last().map(|(n, _)| n + 1).unwrap_or(1);
    let pid = std::process::id();
    let mut made_again = false;
    loop {
        let path = tickets_dir(dir).join(format!("{candidate}.{pid}"));
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Some(Ticket { number: candidate, path: Some(path) }),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => candidate += 1,
            Err(_) if !made_again => {
                made_again = true;
                let _ = fs::create_dir_all(tickets_dir(dir));
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
    wait_in(&slots_dir(), cap(), still_wanted)
}

/// The same wait, in any pool: the global one, or a lane.
fn wait_in(dir: &Path, cap: Option<usize>, still_wanted: impl Fn() -> bool) -> Option<Held> {
    // NO CAP MEANS NO LINE. Taking a number to stand in a queue nobody
    // is holding would be ceremony, and one more file to clean up.
    if cap.is_none() {
        return Some(Held(None));
    }
    let ticket = take_ticket(dir)?;
    wait_with(dir, cap, ticket, still_wanted)
}

/// Stand in line with a number already held, until it is our turn.
fn wait_with(dir: &Path, cap: Option<usize>, ticket: Ticket, still_wanted: impl Fn() -> bool) -> Option<Held> {
    loop {
        // ASKED FIRST, before anything below recreates a directory for a
        // job that nobody can find any more.
        if !still_wanted() {
            return None;
        }
        reclaim_dead_tickets(dir);
        let head = outstanding(dir).first().map(|(n, _)| *n);
        if head == Some(ticket.number) {
            if let Some(held) = try_take(dir, cap) {
                return Some(held); // the ticket is dropped with us, releasing our place
            }
        }
        reclaim_dead(dir);
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// How many slots are held right now, and how many there are.
pub fn busy() -> (usize, Option<usize>) {
    (held_in(&slots_dir()), cap())
}

/// Locks held in a pool, once the dead have been let go of.
fn held_in(dir: &Path) -> usize {
    reclaim_dead(dir);
    fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("lock"))
                .count()
        })
        .unwrap_or(0)
}

// ────────────────────────────────────────────────────────────────────
// LANES — ONE QUEUE PER RESOURCE
// ────────────────────────────────────────────────────────────────────
//
// `slots` caps how many queued jobs run IN ALL. Two deliveries from one
// clone must not overlap — two `git fetch` on the same refs fail on each
// other's lock — and `slots 1` would serialise every test and build with
// them. A lane is the same pool in its own directory: the jobs that name
// it pass one at a time (or n), in filing order, and nothing else waits.
//
// THE LANE IS TAKEN FIRST, THEN THE GLOBAL SLOT. A job waiting its turn
// in a lane holds nothing in the global pool, so a busy lane never
// starves unrelated work; and the global pool never waits on a lane, so
// the two cannot hold each other up.

/// A lane as named on `jbx queue --lane <name>[:<width>]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Lane {
    pub name: String,
    pub width: usize,
}

impl Lane {
    /// `name` or `name:width`. THE NAME BECOMES A DIRECTORY, so it is
    /// held to letters, digits, `_`, `.` and `-` — anything else is a
    /// path somebody did not mean to write.
    pub fn parse(spec: &str) -> Result<Lane, String> {
        let (name, width) = match spec.split_once(':') {
            Some((n, w)) => match w.trim().parse::<usize>() {
                Ok(w) if w >= 1 => (n.trim(), w),
                _ => return Err(format!("`{w}` is not a width: a whole number, 1 or more")),
            },
            None => (spec.trim(), 1),
        };
        let fine = !name.is_empty()
            && !name.starts_with('.')
            && name.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
        if !fine {
            return Err(format!("`{name}` is not a lane name: letters, digits, `_`, `.`, `-`"));
        }
        Ok(Lane { name: name.to_string(), width })
    }

    /// What the supervisor is told: the width already settled.
    pub fn spec(&self) -> String {
        format!("{}:{}", self.name, self.width)
    }

    fn dir(&self) -> PathBuf {
        lane_dir(&self.name)
    }
}

fn lane_dir(name: &str) -> PathBuf {
    slots_dir().join("lanes").join(name)
}

/// SETTLE THE WIDTH WHEN THE JOB IS FILED, and say so if it was not ours.
///
/// A lane in use keeps the width it has: changing it under jobs already
/// running or waiting would let a second delivery in beside the first,
/// which is the one thing the lane is for. An idle lane takes the width
/// it is given. Returns the lane as it will run, and the width that was
/// asked for when it was not the one kept.
pub fn settle(lane: &Lane) -> (Lane, Option<usize>) {
    let dir = lane.dir();
    let _ = fs::create_dir_all(&dir);
    reclaim_dead_tickets(&dir);
    let in_use = held_in(&dir) > 0 || !outstanding(&dir).is_empty();
    let kept = fs::read_to_string(dir.join("width")).ok().and_then(|t| t.trim().parse::<usize>().ok());
    match kept {
        Some(width) if in_use && width != lane.width => {
            (Lane { name: lane.name.clone(), width }, Some(lane.width))
        }
        _ => {
            let _ = fs::write(dir.join("width"), lane.width.to_string());
            (lane.clone(), None)
        }
    }
}

/// How many jobs are in a lane right now: running, and waiting — not
/// counting the supervisor `ours`, which may or may not have taken its
/// number yet. Subtracting one for it guessed, and guessed wrong when it
/// had not.
pub fn lane_load(name: &str, ours: u32) -> (usize, usize) {
    let dir = lane_dir(name);
    reclaim_dead_tickets(&dir);
    let waiting = outstanding(&dir).iter().filter(|(_, pid)| *pid != ours).count();
    (held_in(&dir), waiting)
}

/// A PLACE IN A LANE'S LINE, TAKEN WHEN THE JOB IS FILED.
///
/// The number used to be taken by the supervisor when it started, and
/// filing order held only because processes start fast: on Windows the
/// second job's supervisor came up first and ran first, in a lane whose
/// whole promise is that it does not (the 0.29.0 CI, windows-latest
/// alone). `queue` now takes the number itself — it runs in the order it
/// is called — under its own pid, and hands it to the supervisor by
/// renaming it once the supervisor's pid is known.
pub struct Reservation(PathBuf, u64);

pub fn reserve(lane: &Lane) -> Option<Reservation> {
    let ticket = take_ticket(&lane.dir())?;
    let found = Reservation(ticket.path.clone()?, ticket.number);
    std::mem::forget(ticket); // ours to hand over, not to drop
    Some(found)
}

impl Reservation {
    /// Give the place to the supervisor `pid`. A failure leaves the
    /// reservation under our pid, which the reclaiming of the dead
    /// clears once we exit; the supervisor then takes a number of its own.
    pub fn hand_to(self, lane: &Lane, pid: u32) {
        let _ = fs::rename(&self.0, tickets_dir(&lane.dir()).join(format!("{}.{pid}", self.1)));
    }
}

/// The place `queue` reserved for us, if it did. Looked for briefly: the
/// rename happens just after our own spawn, so it is milliseconds away.
fn adopt(dir: &Path) -> Option<Ticket> {
    let me = std::process::id();
    for _ in 0..50 {
        if let Some((number, _)) = outstanding(dir).into_iter().find(|(_, pid)| *pid == me) {
            return Some(Ticket { number, path: Some(tickets_dir(dir).join(format!("{number}.{me}"))) });
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

/// WAIT FOR A TURN IN A LANE. Held for as long as the job runs.
pub fn wait_for_lane(lane: &Lane, still_wanted: impl Fn() -> bool) -> Option<Held> {
    let dir = lane.dir();
    let ticket = adopt(&dir).or_else(|| take_ticket(&dir))?;
    wait_with(&dir, Some(lane.width), ticket, still_wanted)
}

/// Where the supervisor `pid` stands in a lane's line: `(place, of)`,
/// counted from 1. `None` once it holds its turn, or if it never stood.
pub fn lane_position(name: &str, pid: u32) -> Option<(usize, usize)> {
    let line = outstanding(&lane_dir(name));
    let at = line.iter().position(|(_, p)| *p == pid)?;
    Some((at + 1, line.len()))
}
