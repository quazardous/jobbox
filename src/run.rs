//! RUNNING A LINE, AND LETTING GO OF IT WHEN IT TURNS OUT TO BE LONG.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use crate::input;
use crate::store::{self, Record};

/// How often the growing log is copied to our own output — CLOSE AT
/// FIRST, THEN RELAXED.
///
/// The output travels through a file rather than a pipe, which is what
/// lets it keep being written after we are gone; the price of that is a
/// poll. A flat twenty milliseconds looks harmless and is not: MEASURED,
/// it put 20 ms on every command on the machine, because a line that
/// takes one millisecond still waits a whole tick to be noticed. Since
/// most lines are short, that tick was the wrapper's entire cost.
///
/// So the first hundred milliseconds are watched closely and the rest is
/// not. A long line is not made slower by being noticed 20 ms late; a
/// short one is made 20 ms slower by exactly that.
fn poll_after(elapsed: f64) -> Duration {
    if elapsed < 0.1 {
        Duration::from_micros(500)
    } else if elapsed < 1.0 {
        Duration::from_millis(5)
    } else {
        Duration::from_millis(20)
    }
}

/// The shell that runs the line — the platform's own, not a choice.
///
/// The line was written for the shell the caller uses; handing it to a
/// different one would change what it means. On Windows that is `cmd`,
/// whose `/C` takes the rest as the command.
fn shell(line: &str) -> Command {
    match shell_program() {
        Shell::Posix(program) => {
            let mut cmd = Command::new(program);
            cmd.arg("-c").arg(line);
            cmd
        }
        Shell::Cmd => {
            let mut cmd = Command::new("cmd");
            cmd.arg("/C").arg(line);
            cmd
        }
    }
}

pub enum Shell {
    Posix(String),
    Cmd,
}

/// WHICH SHELL RUNS THE LINE — and it must be the one the line was
/// WRITTEN for.
///
/// THE HOOK QUOTES POSIX. It writes `jbx run -- '<line>'`, with single
/// quotes, because the harness that hands us the line speaks a POSIX
/// shell — including on Windows, where Claude Code drives Git Bash. This
/// used to hand that line to `cmd /C` there, which understands neither
/// the syntax nor the quotes: the two halves of the tool assumed
/// different shells. Nobody had run it on Windows to find out.
///
/// So `bash` wins wherever it exists, on every platform. `cmd` is the
/// fallback for a Windows machine without it, and `shell:` in the
/// configuration settles it for anybody whose setup we guessed wrong.
pub fn shell_program() -> Shell {
    static CHOSEN: OnceLock<Shell> = OnceLock::new();
    // A `OnceLock` because this is on the path of every wrapped command,
    // and scanning the PATH once per process is already more than the
    // question deserves.
    match CHOSEN.get_or_init(|| {
        if let Some(named) = crate::config::shell() {
            return if named == "cmd" { Shell::Cmd } else { Shell::Posix(named) };
        }
        // THE PATH WE CHECKED IS THE PATH WE RUN. Returning the bare
        // name let `Command` do its own lookup, which does not skip
        // `System32` — so the check excluded the WSL launcher and the
        // spawn picked it up again, two lines later. Verifying one thing
        // and using another is its own kind of bug, and this is what it
        // looks like.
        if let Some(found) = find_on_path("bash") {
            return Shell::Posix(found.display().to_string());
        }
        // GIT FOR WINDOWS IS OFTEN INSTALLED AND OFTEN NOT ON THE PATH.
        // Looking where it lives costs two `stat`s and is the difference
        // between a working install and `cmd`, which has no `sleep` and
        // understands none of the quoting the hook writes.
        #[cfg(windows)]
        for candidate in [
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
        ] {
            if std::path::Path::new(candidate).is_file() {
                return Shell::Posix(candidate.into());
            }
        }
        if cfg!(windows) {
            Shell::Cmd
        } else {
            Shell::Posix("sh".into())
        }
    }) {
        Shell::Posix(p) => Shell::Posix(p.clone()),
        Shell::Cmd => Shell::Cmd,
    }
}

/// Whether a program is reachable, without spawning it.
///
/// Running it to find out would cost a process on every wrapped command,
/// which is the one budget this path does not have.
fn find_on_path(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        // `C:\Windows\System32\bash.exe` IS NOT A SHELL. It is the WSL
        // launcher, and on a machine with no distribution installed it
        // answers every command with "Windows Subsystem for Linux has no
        // installed distributions" — in UTF-16, which is how it was
        // finally recognised. Found by running the suite on a real
        // Windows runner for the first time; no amount of reading would
        // have shown it.
        if cfg!(windows) && in_system32(dir.as_path()) {
            return None;
        }
        let plain = dir.join(program);
        if plain.is_file() {
            return Some(plain);
        }
        let exe = dir.join(format!("{program}.exe"));
        exe.is_file().then_some(exe)
    })
}

fn in_system32(dir: &std::path::Path) -> bool {
    dir.to_string_lossy().to_ascii_lowercase().contains("system32")
}

/// Detach a child from us, so that it outlives the process that started it.
fn detach(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // ITS OWN PROCESS GROUP, so a Ctrl-C aimed at us does not reach
        // it and so killing it later means killing one group, whatever
        // the line spawned in the meantime.
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // NO WINDOW RATHER THAN NO CONSOLE. This was tried as the fix
        // for the silent Windows failures and IT CHANGED NOTHING — the
        // cause was the log handle, next door in `supervise`. It is kept
        // because it says what we actually want: nothing pops up, a
        // Windows process already outlives its parent, and the new
        // process group still keeps a Ctrl-C from reaching the job.
        // `DETACHED_PROCESS` additionally denies the process a console
        // at all, which we never needed and which nothing here relies
        // on.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
    }
}

/// THE SUPERVISOR: it runs the line, waits for it, and records its code.
///
/// IT EXISTS BECAUSE WE WILL NOT BE THERE TO REAP THE CHILD. Once the
/// front process has let go, nobody is left to learn the exit status —
/// an orphan's code is collected by init and thrown away. So a second
/// copy of this binary stays behind for exactly one purpose: to wait,
/// and to write down the number.
///
/// It is the same binary re-invoked rather than a shell trap, because a
/// trap is a shell feature and `cmd.exe` has no equivalent. One
/// mechanism on both platforms beats two that drift apart.
pub fn supervise(id: &str, after: f64, queued: bool, fg: bool, line: &str) -> i32 {
    // OPENED FOR WRITING AND SEEKED, NOT APPENDED — and on Windows that
    // is the difference between a working program and twenty silent
    // failures.
    //
    // `append(true)` asks Windows for `FILE_APPEND_DATA` and nothing
    // else: a handle that cannot be seeked and carries no read access.
    // `cmd /C` never notices, because it only ever calls WriteFile. The
    // MSYS runtime behind Git Bash adopts an inherited handle and does
    // more with it than that, and on an append-only one it fails —
    // every write from the line fails, `echo` returns non-zero, bash
    // exits 1, and the error explaining it goes to the same broken
    // handle. An empty log and a code of 1, with nothing to read.
    //
    // MEASURED, after two wrong guesses. `jbx run -- 'echo x > out.txt'`
    // wrote `out.txt` and `jbx run -- 'exit 7'` recorded 7: the shell
    // was running the line perfectly and only its output was lost.
    //
    // Standard output and standard error are two clones of this handle
    // and therefore share one file position, so they still interleave
    // the way they would on a terminal.
    let log = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        // NEVER TRUNCATE. The front creates this file before we exist;
        // emptying it here would throw away whatever a queued job's
        // waiting had already put in it.
        .truncate(false)
        .open(store::log_path(id))
    {
        Ok(mut f) => {
            use std::io::Seek;
            let _ = f.seek(std::io::SeekFrom::End(0));
            f
        }
        Err(e) => {
            eprintln!("jbx: cannot open the log: {e}");
            return 1;
        }
    };
    let err_side = match log.try_clone() {
        Ok(f) => f,
        Err(_) => return 1,
    };
    // BOTH STREAMS INTO ONE FILE, on purpose: they interleave the way
    // they would have on a terminal, and a reader gets one story instead
    // of two that have to be zipped back together by guesswork.
    // THE WAIT HAPPENS HERE, BEHIND THE CALLER. `queue` handed the job
    // over and left; standing in line is this process's work, not
    // theirs. The slot is released when `held` falls out of scope, so it
    // is released whichever way this function ends.
    let _held = if queued {
        let held = crate::slots::wait_for_one();
        let _ = std::fs::write(store::started_path(id), "");
        Some(held)
    } else {
        None
    };

    let mut cmd = shell(line);
    // THE LINE KEEPS THE CALLER'S OWN STANDARD INPUT.
    //
    // Closing it would be a wrapper deciding, for every command it
    // wraps, that none of them reads anything — and `sort`, `cat` and
    // every filter in a pipeline read. Under a harness this changes
    // nothing (the input is already at end of file); everywhere else it
    // is the difference between wrapping a command and altering it.
    cmd.stdin(Stdio::inherit()).stdout(Stdio::from(log)).stderr(Stdio::from(err_side));
    // THE LINE IS TOLD IT IS ALREADY WRAPPED, so that a `jbx run` inside
    // it steps aside instead of making a second job. See `run_inner`.
    cmd.env("JBX_WRAPPED", id);
    let began = store::now();
    let code = match cmd.spawn() {
        Ok(mut child) => match child.wait() {
            Ok(status) => exit_code(status),
            Err(_) => 1,
        },
        Err(e) => {
            eprintln!("jbx: cannot start the line: {e}");
            127
        }
    };
    let took = store::now() - began;
    crate::stats::record(&crate::stats::fingerprint(line), took, after, fg, code);
    // ONLY A DETACHED JOB IS ANNOUNCED, and `took > after` is exactly
    // that — no coordination with the front needed, which is the point:
    // asking it would mean a handshake with a process that has already
    // gone. A line that finished in time was never a job, and announcing
    // every command would be a notification per shell call.
    if queued || took > after {
        let intent = store::read_record(id)
            .map(|r| r.intent)
            .unwrap_or_else(|| store::intent_of(line));
        crate::signals::deposit(
            id,
            code,
            &intent,
            &store::log_path(id).display().to_string(),
            &store::client(),
        );
    }

    // THE EXIT CODE IS WRITTEN LAST, AND THAT ORDER IS THE CONTRACT.
    //
    // The front returns the moment it sees this file, and `wait` and `fg`
    // end on it. So everything a reader might look at next — the
    // measurement, the ending in the mailbox — has to be on disk before
    // it appears. It used to be written first, and a caller could read
    // back a job it had just watched finish and find no reading for it.
    if let Err(e) = store::write_code(id, code) {
        // THE ONE FAILURE THAT LOOKS LIKE A KILLED JOB. Without the
        // code file a reader can only say "gone, no exit code", which
        // is what a killed process leaves — so the reason goes into the
        // log, the only thing here that outlives this process.
        use std::io::Write;
        if let Ok(mut sink) = OpenOptions::new().append(true).open(store::log_path(id)) {
            let _ = writeln!(sink, "\njbx: the line exited {code}, but its exit code could not be recorded: {e}");
        }
    }
    // THE READING IS TAKEN HERE AND NOWHERE ELSE. Only this process
    // knows both how long the line really took and what it returned —
    // the front let go of it long before either was true.
    code
}

/// A status as a number, including the signal that ended it.
///
/// A killed process has no exit code at all. `128 + signal` is the
/// convention every shell already uses for it, so the number stays
/// readable by everything downstream rather than becoming a special case
/// only this tool understands.
fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return 128 + sig;
        }
    }
    1
}

/// THE FRONT: run the line, hand its output through as it comes, and
/// detach it if it is still going after `after` seconds.
///
/// THE EXIT CODE IS NEVER LOST, ONLY DEFERRED. Finished in time, we
/// return it unchanged, as though this had never been here. Detached,
/// there is nothing to return YET — so we return 0, which is the truth
/// about what THIS process did, and the real code is written where
/// `status` and `wait` read it.
pub fn run(after: f64, line: &str, intent: Option<&str>) -> i32 {
    run_inner(after, line, false, intent)
}

fn run_inner(after: f64, line: &str, fg: bool, intent: Option<&str>) -> i32 {
    if line.trim().is_empty() {
        eprintln!("jbx: nothing to run. `jbx run -- '<command line>'`");
        return 2;
    }

    // ALREADY INSIDE A WRAPPED LINE — SO SOMEBODY ELSE HOLDS THIS JOB.
    //
    // The hook wraps every command. When the command it wrapped is
    // itself a `jbx run`, there were TWO jobs: the outer one, whose id
    // is the one announced, and the inner one doing the work. The outer
    // ends in seconds with `exit 0` and a log holding nothing but the
    // inner's detachment message — which reads exactly like a finished
    // job, while the real one runs on under an id nobody was told.
    //
    // Reported after four wrong ids in one session (#2066): a suite
    // declared finished, a `kill` aimed at the wrong thing, and a `wait`
    // that returned at once so the next command started underneath the
    // work. It needs no pipe, and it explains the first report too.
    //
    // The outer wrapper already gives this line a log, an exit code, a
    // detachment and an announcement. A second one adds nothing and
    // costs the truth about which id to trust — so the inner one steps
    // aside, exactly as it does for a terminal.
    if std::env::var_os("JBX_WRAPPED").is_some() {
        let mut cmd = shell(line);
        return match cmd.status() {
            Ok(status) => exit_code(status),
            Err(e) => {
                eprintln!("jbx: {e}");
                127
            }
        };
    }

    // A TTY MEANS A HUMAN IS WATCHING, AND WE GET OUT OF THE WAY.
    //
    // Everything that makes wrapping safe under a harness is a fact
    // about a terminal that is not there: nothing can prompt, nothing
    // needs to stay on screen, the output was already being captured.
    // With a terminal all three come back at once — so the line goes
    // straight to a shell and this wrapper stops existing.
    if io::stdout().is_terminal_like() || io::stdin().is_terminal_like() {
        let mut cmd = shell(line);
        return match cmd.status() {
            Ok(status) => exit_code(status),
            Err(e) => {
                eprintln!("jbx: {e}");
                127
            }
        };
    }

    let dir = store::dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("jbx: cannot use {}: {e}", dir.display());
        return 2;
    }
    store::forget_older_than(24.0);
    crate::stats::forget_older_than(90.0);

    let id = store::mint();
    let log = store::log_path(&id);
    if File::create(&log).is_err() {
        eprintln!("jbx: cannot create {}", log.display());
        return 2;
    }

    let mut cmd = Command::new(std::env::current_exe().unwrap_or_else(|_| "jbx".into()));
    cmd.arg("supervise").arg(&id).arg("--after").arg(after.to_string());
    if fg {
        cmd.arg("--fg");
    }
    cmd.arg("--").arg(line);
    // THE SUPERVISOR GETS NEITHER OF OUR OUTPUT STREAMS, AND THIS IS
    // LOAD-BEARING.
    //
    // The harness reads our standard output until it closes. A detached
    // child holding the same pipe keeps it open after we exit, so the
    // tool would go on waiting for a process it cannot see — the exact
    // hang this whole thing exists to remove, reintroduced by
    // inheritance. Its streams are closed; the line's output goes to the
    // log, which is where it can still be written from after we are
    // gone.
    cmd.stdin(Stdio::inherit()).stdout(Stdio::null()).stderr(Stdio::null());
    detach(&mut cmd);
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("jbx: cannot start the supervisor: {e}");
            return 2;
        }
    };

    let record = Record {
        id: id.clone(),
        queued: false,
        mirror_cut: false,
        detached: Some(false),
        pid: child.id(),
        command: line.to_string(),
        // WHAT THE CALLER SAID IT WAS DOING, when they said anything.
        //
        // The harness already asks for a one-line description of every
        // command and hands it to the hook — so a job can be named by
        // whoever ran it rather than by the first four words of its own
        // text. `make simulation-dag ARGS=…` becomes "run the DAG
        // simulation after the rope fork", which is what a list is read
        // for three hours later.
        intent: intent
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| t.chars().take(72).collect())
            .unwrap_or_else(|| store::intent_of(line)),
        started: store::now(),
        client: store::client(),
        cwd: std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(),
        project: crate::stats::project().1,
    };
    // WRITTEN NOW AND NOT AT DETACHMENT, so that a front process killed
    // by a harness timeout still leaves something findable behind. It is
    // removed again if the line finishes in time.
    let _ = store::write_record(&record);

    let started = store::now();
    let mut reader = match File::open(&log) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("jbx: cannot follow {}: {e}", log.display());
            return 2;
        }
    };
    let mut buf = [0u8; 16 * 1024];
    // WHETHER WHOEVER WAS READING US WENT AWAY.
    let mut cut = false;
    // SINCE WHEN NOTHING HAS HAPPENED. A single sample at the threshold
    // cannot tell a stuck process from a busy one — it catches every
    // pipeline stage mid-read. These two say whether it has LASTED, and
    // they are gathered while we are polling anyway.
    let mut quiet_since = store::now();
    let mut reading_since: Option<f64> = None;
    let mut last_look = 0.0_f64;
    loop {
        let moved = drain(&mut reader, &mut buf, &mut cut);
        if moved > 0 {
            quiet_since = store::now();
        }
        // WALKING /proc IS NOT FREE, and nothing here changes in twenty
        // milliseconds. Twice a second is far more often than any
        // conclusion drawn from it needs.
        let now = store::now();
        if now - last_look >= 0.5 {
            last_look = now;
            if input::reading_its_input(child.id()).is_some() {
                reading_since.get_or_insert(now);
            } else {
                reading_since = None;
            }
        }
        let done = store::code_path(&id).exists();
        if done {
            // ONE LAST DRAIN AFTER THE CODE APPEARS. The code is written
            // by the supervisor only once the line has exited, so
            // everything the line ever printed is already in the file —
            // but not necessarily already read by us.
            while drain(&mut reader, &mut buf, &mut cut) > 0 {}
            let code = std::fs::read_to_string(store::code_path(&id))
                .ok()
                .and_then(|t| t.trim().parse::<i32>().ok())
                .unwrap_or(1);
            for path in [store::record_path(&id), store::code_path(&id), log] {
                let _ = std::fs::remove_file(path);
            }
            if cut {
                // THE EXIT CODE IS RIGHT AND THE VIEW WAS NOT. Saying so
                // costs one line and saves the conclusion that what you
                // read was all of it.
                eprintln!("jbx: whoever was reading this output stopped early, so what you saw\n\
                           is only part of it. The command itself ran to the end.");
            }
            return code;
        }
        if store::now() - started >= after {
            let now = store::now();
            // ONE LAST LOOK BEFORE SPEAKING. The line may have finished
            // between the check above and this one, and announcing a job
            // that is already done is how the ticket's worst case read:
            // "it will not finish on its own", about a deployment that
            // had.
            if store::code_path(&id).exists() {
                continue;
            }
            // THE LAUNCHER IS LETTING GO, and the job has to say so:
            // until now it read `running` exactly like one still being
            // held, and the two want different things done about them.
            //
            // The cut mirror is written here too — with `2>&1 | head`
            // both streams are the closed pipe, so `status` is the only
            // place a reader will ever learn their view was partial.
            if let Some(mut r) = store::read_record(&id) {
                r.detached = Some(true);
                r.mirror_cut = cut;
                let _ = store::write_record(&r);
            }
            let observation = Observation {
                mirror_cut: cut,
                reading_for: reading_since.map(|t| now - t).unwrap_or(0.0),
                quiet_for: now - quiet_since,
            };
            return announce(&id, after, observation);
        }
        if moved == 0 {
            std::thread::sleep(poll_after(store::now() - started));
        }
    }
}

/// Copy whatever has appeared in the log since last time, straight out.
///
/// FLUSHED EVERY TIME, DELIBERATELY. Rust's standard output holds back
/// until a line is complete, which turns a progress bar — the one kind
/// of output whose entire job is to arrive early — into nothing at all
/// until the command ends.
fn drain(reader: &mut File, buf: &mut [u8], cut: &mut bool) -> usize {
    match reader.read(buf) {
        Ok(0) | Err(_) => 0,
        Ok(n) => {
            let out = io::stdout();
            let mut out = out.lock();
            // A CLOSED PIPE IS NOT AN ERROR — `… | head` closes it on
            // purpose and the line behind us is entitled to go on — but
            // it IS worth remembering. What we print is a mirror of the
            // log; a mirror cut short reads exactly like the whole story
            // (#2066), and somebody concluded a suite had finished.
            if out.write_all(&buf[..n]).is_err() {
                *cut = true;
            }
            let _ = out.flush();
            n
        }
    }
}

/// What was observed while waiting, and for how long.
///
/// DURATIONS, NOT VERDICTS. Whether "stopped reading its input for
/// twelve seconds" means anything is a judgement, and the one this code
/// used to make was wrong often enough to cost a deployment.
struct Observation {
    /// Whether whoever was reading our output went away before the end.
    mirror_cut: bool,
    /// How long the subtree has been continuously stopped reading its
    /// own standard input; zero when it is not, or when we cannot see.
    reading_for: f64,
    /// How long since anything was written to the log.
    quiet_for: f64,
}

/// Before an observation is worth passing on at all.
///
/// Long enough that a slow pipeline stage has had its turn. It is still
/// only a hint — `sleep 60 | cat` would trip it, and finish.
const WORTH_MENTIONING: f64 = 10.0;

/// Say that the line was detached, and how to pick it up again.
///
/// THE MESSAGE IS THE PRODUCT HERE. Whoever reads it — a person or an
/// agent — has just been handed back a shell that is not finished, and
/// what they do next depends on being told plainly what happened, that
/// nothing was lost, and which verb answers "and now?".
///
/// IT NO LONGER PREDICTS. It used to say "it will not finish on its own"
/// whenever something in the subtree was mid-`read` — which is what
/// every pipeline stage and every `docker` client relaying a terminal
/// looks like. On a deployment that had already succeeded it advised
/// killing or re-running: one loses the result, the other does it twice
/// (#2063). What is left is what was seen, with its duration.
fn announce(id: &str, after: f64, seen: Observation) -> i32 {
    let out = io::stdout();
    let mut out = out.lock();
    let _ = writeln!(
        out,
        "jbx: this passed {after:.0}s, so it is now in the BACKGROUND — detached as {id}.\n\
         Nothing was lost. It is still running, and still printing to its log.\n\
         \n\
         DO NOT SIT AND WAIT FOR IT. You will be told when it ends, on a later turn —\n\
         waiting here is the exact cost jbx exists to remove. Go and do something else.\n"
    );
    if seen.reading_for >= WORTH_MENTIONING && seen.quiet_for >= WORTH_MENTIONING {
        // SAID AS AN OBSERVATION, AND ONLY ONCE IT HAS LASTED. A
        // pipeline waiting on a slow producer looks exactly like this,
        // so the reader is told what was seen and left to judge it.
        let _ = writeln!(
            out,
            "It has printed nothing for {:.0}s and has been reading its standard input\n\
             throughout. That is often ordinary — a pipeline stage waiting on a slow\n\
             producer looks the same. But if it is waiting for input nobody here can\n\
             give it, re-running with `… < /dev/null`, or with whatever flag makes it\n\
             non-interactive, will settle it.",
            seen.quiet_for
        );
    }
    let _ = writeln!(
        out,
        "\x20 jbx how {id}   what you can do with it   ·   jbx why   why it works this way"
    );
    if seen.mirror_cut {
        // ON STDERR, because stdout is precisely what stopped being
        // read. This is the case that cost half an hour: a mirror cut
        // short reads exactly like a finished job (#2066).
        eprintln!("jbx: whoever was reading this output stopped early — what you saw is a\n\
                   truncated MIRROR of the log, not the end of the job. {id} is still\n\
                   running; `jbx status {id}` is the real answer.");
    }
    let _ = out.flush();
    0
}

/// Whether a stream is a terminal.
///
/// `IsTerminal` is in the standard library on both platforms, but this
/// keeps the call in one place with a name that says what it decides —
/// the whole "get out of the way" rule turns on it.
trait TerminalLike {
    fn is_terminal_like(&self) -> bool;
}
impl<T: std::io::IsTerminal> TerminalLike for T {
    fn is_terminal_like(&self) -> bool {
        self.is_terminal()
    }
}

/// `jbx queue <intent> -- <line>` — HAND WORK OVER BEFORE IT STARTS.
///
/// THE OTHER DOOR, and the only one where a cap means anything. `run`
/// wraps a command that was going to run either way; this takes work
/// that has NOT started, so it can be made to wait its turn — and a loop
/// that files fifty jobs does not start fifty at once.
///
/// THE INTENT IS MANDATORY HERE AND NOWHERE ELSE. `run` names a line
/// after the fact, from its first words, because nobody chose to
/// background it. Somebody choosing to has a name in mind, and three
/// words at that moment are what makes a list readable three hours later.
pub fn queue(intent: &str, line: &str) -> i32 {
    if intent.trim().is_empty() || line.trim().is_empty() {
        eprintln!("jbx: `jbx queue <intent> -- '<line>'` — both are required");
        return 2;
    }
    let dir = store::dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("jbx: cannot use {}: {e}", dir.display());
        return 2;
    }
    let id = store::mint();
    if File::create(store::log_path(&id)).is_err() {
        eprintln!("jbx: cannot create the log for {id}");
        return 2;
    }
    let mut cmd = Command::new(std::env::current_exe().unwrap_or_else(|_| "jbx".into()));
    cmd.arg("supervise")
        .arg(&id)
        .arg("--after")
        .arg("0")
        .arg("--queued")
        .arg("--")
        .arg(line);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    detach(&mut cmd);
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("jbx: cannot start the supervisor: {e}");
            return 2;
        }
    };
    let record = Record {
        id: id.clone(),
        queued: true,
        mirror_cut: false,
        // NOBODY HOLDS A QUEUED JOB — that is the point of handing it
        // over. It is in the background from the moment it starts.
        detached: Some(true),
        pid: child.id(),
        command: line.to_string(),
        intent: intent.to_string(),
        started: store::now(),
        client: store::client(),
        cwd: std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(),
        project: crate::stats::project().1,
    };
    let _ = store::write_record(&record);
    // THE ID FIRST AND ALONE ON ITS LINE: it is what every other verb
    // takes, and a caller in a script should not have to read prose to
    // find it.
    outln!("{id}");

    // AND THEN, IN WORDS, WHETHER IT STARTED. A verb that answers with
    // an id and nothing else lets somebody believe the work has begun —
    // and a job held back by a full queue looks exactly like one that is
    // already running, until they go and look.
    let (busy, cap) = crate::slots::busy();
    let ahead = store::all()
        .iter()
        .filter(|r| matches!(store::state_of(r), store::State::Queued))
        .count();
    match cap {
        Some(cap) if busy >= cap => outln!(
            "  NOT STARTED — all {cap} slots are busy, {ahead} waiting. It begins when one frees.\n\
             \x20 jbx ps    what is holding them"
        ),
        Some(cap) => outln!("  a slot was free ({busy} of {cap} busy) — it starts now."),
        None => outln!("  no cap on how many run at once — it starts now."),
    }
    0
}

/// `jbx fg -- <line>` — RUN IT, AND NEVER LET GO.
///
/// THE DELIBERATE FOREGROUND. Everything else here exists to stop a
/// caller standing still; this is how a caller says "I have thought
/// about it, and I need the answer before I can go on". Saying it out
/// loud is the point: `jbx stats` counts what it cost, so a habit of
/// reaching for it shows up as time that was never saved.
pub fn foreground(line: &str, intent: Option<&str>) -> i32 {
    run_inner(f64::INFINITY, line, true, intent)
}

/// `jbx fg <id>` — BRING A DETACHED JOB BACK.
///
/// The other half of the same word. A job that was let go of and now
/// turns out to be the thing you are waiting for is picked back up here:
/// what it has already printed, then what it prints next, and its exit
/// code when it lands.
///
/// THE BLOCK IS WRITTEN DOWN, exactly as `wait` writes it down. Standing
/// here is standing still whichever verb you typed, and a measurement
/// that only counted one of them would flatter the tool.
pub fn attach(id: &str) -> i32 {
    let Some(record) = store::read_record(id) else {
        eprintln!("jbx: {id} is unknown");
        return 1;
    };
    let began = store::now();
    let mut reader = match File::open(store::log_path(id)) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("jbx: cannot read the log of {id}: {e}");
            return 1;
        }
    };
    let mut buf = [0u8; 16 * 1024];
    let mut cut = false;
    let code = loop {
        let moved = drain(&mut reader, &mut buf, &mut cut);
        match store::settled_state(&record) {
            store::State::Finished { code } => {
                // EVERYTHING IT EVER PRINTED, not just what arrived
                // while we watched: the exit code appears only once the
                // line has exited, so the rest of the log is already
                // there to be read.
                while drain(&mut reader, &mut buf, &mut cut) > 0 {}
                break code;
            }
            store::State::Lost => {
                while drain(&mut reader, &mut buf, &mut cut) > 0 {}
                eprintln!("jbx: {id} ended without leaving an exit code");
                break 1;
            }
            _ => {
                if moved == 0 {
                    std::thread::sleep(POLL_SLOW);
                }
            }
        }
    };
    crate::stats::record_wait(store::now() - began);
    code
}

/// How often a re-attached job's log is checked. Slower than the front's
/// first moments on purpose: this one has already been running a while,
/// so there is no short command whose whole cost is one tick.
const POLL_SLOW: Duration = Duration::from_millis(50);
