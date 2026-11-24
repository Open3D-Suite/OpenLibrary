// SPDX-License-Identifier: GPL-3.0-or-later
//! Jobs of a 3ds Max in the background (`3dsmaxbatch.exe`), which never touch the 3ds Max the person works in.
//!
//! A [`Job`] is a function of `bridge/olib_batch.ms` and a line of work per file. [`run`] writes the scripts and
//! the lines in a folder of the app, starts 3ds Max and reads its log. Some old files leave 3ds Max hung when it
//! opens them without its window: a file that does not move in [`Job::stall`] is given up, that 3ds Max is
//! closed and another one goes on with the rest.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CORE: &[u8] = include_bytes!("../bridge/olib_bridge/olib_core.ms");
const BATCH: &[u8] = include_bytes!("../bridge/olib_batch.ms");
/// Set for the 3ds Max in the background, so that a bridge installed in it does not start.
const VARIABLE: &str = "OPENLIBRARY_BATCH";
/// What 3ds Max may take to start, besides the first file.
const STARTUP: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(500);

/// The word for a job's failure when 3ds Max closed, or was closed, on that file.
pub const STUCK: &str = "stuck";

/// Work for a 3ds Max in the background.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    /// The MAXScript function of `olib_batch.ms`; it takes the jobs file and the log file.
    pub function: &'static str,
    /// Its other arguments.
    pub arguments: Vec<f64>,
    /// One line per file, with its fields.
    pub lines: Vec<Vec<String>>,
    /// How long one file may take without the log moving.
    pub stall: Duration,
}

/// What 3ds Max wrote about a line: its fields, or the reason it failed.
pub type Outcome = Result<Vec<String>, String>;

/// A 3ds Max in the background; a stand-in in the tests.
pub trait Process {
    fn ended(&mut self) -> bool;
    fn kill(&mut self);
}

struct MaxProcess(Child);

impl Process for MaxProcess {
    fn ended(&mut self) -> bool {
        !matches!(self.0.try_wait(), Ok(None))
    }

    /// Closes 3dsmaxbatch and the 3ds Max it starts: closing only the first leaves the second alive.
    fn kill(&mut self) {
        if !self.ended() {
            let taskkill = crate::platform::system_program("taskkill.exe");
            let _ = quiet(Command::new(taskkill).args(["/PID", &self.0.id().to_string(), "/T", "/F"])).status();
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

impl Drop for MaxProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

/// A command without a console window and without anything to read or write.
pub fn quiet(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
}

/// Every `3dsmaxbatch.exe` there may be, from the environment and Program Files: (year, folder), newest first.
fn installs(variables: impl Iterator<Item = (String, String)>, program_files: &[PathBuf]) -> Vec<(u32, PathBuf)> {
    let mut found: Vec<(u32, PathBuf)> = variables
        .filter_map(|(name, value)| {
            let name = name.to_ascii_uppercase();
            Some((name.strip_prefix("ADSK_3DSMAX_X64_")?.parse().ok()?, PathBuf::from(value)))
        })
        .collect();
    for base in program_files {
        for entry in fs::read_dir(base.join("Autodesk")).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(year) = name.strip_prefix("3ds Max ").and_then(|y| y.parse().ok()) {
                found.push((year, entry.path()));
            }
        }
    }
    found.sort();
    found.reverse();
    found.dedup();
    found
}

/// `3dsmaxbatch.exe` of the newest 3ds Max installed.
pub fn find() -> Option<PathBuf> {
    let program_files: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432"].iter().filter_map(std::env::var_os).map(PathBuf::from).collect();
    let variables = std::env::vars_os().filter_map(|(n, v)| Some((n.into_string().ok()?, v.into_string().ok()?)));
    installs(variables, &program_files).into_iter().map(|(_, dir)| dir.join("3dsmaxbatch.exe")).find(|exe| exe.is_file())
}

/// A path in a MAXScript text: forward slashes, and no quote (Windows allows none in a name).
fn ms_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/").replace('"', "")
}

fn bom(text: &str) -> Vec<u8> {
    let mut out = vec![0xEF, 0xBB, 0xBF];
    out.extend_from_slice(text.as_bytes());
    out
}

/// Writes the scripts, the lines and an empty log; the script to run and the log.
fn prepare(dir: &Path, job: &Job, lines: &[Vec<String>]) -> io::Result<(PathBuf, PathBuf)> {
    fs::create_dir_all(dir)?;
    let (core, batch, jobs, log, script) =
        (dir.join("olib_core.ms"), dir.join("olib_batch.ms"), dir.join("jobs.txt"), dir.join("log.txt"), dir.join("run.ms"));
    fs::write(&core, CORE)?;
    fs::write(&batch, BATCH)?;
    fs::write(&log, b"")?;
    let text: String = lines.iter().map(|fields| format!("{}\r\n", fields.iter().map(|f| one_line(f)).collect::<Vec<_>>().join("\t"))).collect();
    fs::write(&jobs, bom(&text))?;
    let arguments: String = job.arguments.iter().map(|a| format!(" {a}")).collect();
    let call = format!("{} \"{}\" \"{}\"{arguments}", job.function, ms_path(&jobs), ms_path(&log));
    fs::write(&script, bom(&format!("fileIn \"{}\"\r\nfileIn \"{}\"\r\n{call}\r\n", ms_path(&core), ms_path(&batch))))?;
    Ok((script, log))
}

/// A field without what separates fields and lines (no path has them; a path read from a file might).
fn one_line(field: &str) -> String {
    field.replace(['\t', '\r', '\n'], " ")
}

/// The lines of the log that are whole, in order.
fn read_log(log: &Path) -> Vec<Outcome> {
    let Ok(data) = fs::read(log) else { return Vec::new() };
    let text = String::from_utf8_lossy(&data).replace('\u{feff}', "");
    let whole = text.rfind('\n').map_or("", |end| &text[..end]);
    whole
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim_end_matches('\r').split('\t').map(str::to_string);
            let state = parts.next()?;
            parts.next()?.parse::<usize>().ok()?;
            let fields: Vec<String> = parts.collect();
            match state.as_str() {
                "ok" => Some(Ok(fields)),
                "fail" => Some(Err(fields.join(" "))),
                _ => None,
            }
        })
        .collect()
}

fn start(maxbatch: &Path, script: &Path) -> io::Result<Box<dyn Process>> {
    let mut command = Command::new(maxbatch);
    command.arg(script).env(VARIABLE, "1");
    Ok(Box::new(MaxProcess(quiet(&mut command).spawn()?)))
}

/// Runs a job to its end; `done` is told each line as it ends, by its number from 0. Returns an error when
/// 3ds Max cannot be started, or ends again and again without doing anything.
pub fn run(maxbatch: &Path, dir: &Path, job: &Job, cancelled: &dyn Fn() -> bool, done: &mut dyn FnMut(usize, Outcome)) -> Result<(), String> {
    let result = run_with(dir, job, cancelled, done, &mut |script| start(maxbatch, script), POLL);
    let _ = fs::remove_dir_all(dir); // only this job's scripts and log
    result
}

fn run_with(
    dir: &Path,
    job: &Job,
    cancelled: &dyn Fn() -> bool,
    done: &mut dyn FnMut(usize, Outcome),
    launch: &mut dyn FnMut(&Path) -> io::Result<Box<dyn Process>>,
    poll: Duration,
) -> Result<(), String> {
    let mut first = 0;
    let mut empty_runs = 0;
    while first < job.lines.len() && !cancelled() {
        let (script, log) = prepare(dir, job, &job.lines[first..]).map_err(|e| e.to_string())?;
        let mut process = launch(&script).map_err(|e| e.to_string())?;
        let mut seen = 0;
        let mut deadline = Instant::now() + STARTUP + job.stall;
        let mut stalled = false;
        loop {
            let ended = process.ended(); // asked before the log is read, so that its last lines count
            let outcomes = read_log(&log);
            if outcomes.len() > seen {
                deadline = Instant::now() + job.stall;
                for outcome in outcomes.into_iter().skip(seen).take(job.lines.len() - first - seen) {
                    done(first + seen, outcome);
                    seen += 1;
                }
            }
            if ended || cancelled() {
                break;
            }
            if Instant::now() > deadline {
                stalled = true;
                break;
            }
            std::thread::sleep(poll);
        }
        process.kill();
        first += seen;
        if cancelled() || first >= job.lines.len() {
            break;
        }
        // closed before the end: that file is given up and the rest goes on
        empty_runs = if seen == 0 && !stalled { empty_runs + 1 } else { 0 };
        done(first, Err(STUCK.to_string()));
        first += 1;
        if empty_runs >= 2 {
            return Err("3ds Max".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openlibrary-maxbatch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn job(lines: usize) -> Job {
        let lines = (1..=lines).map(|n| vec![format!("S:\\m{n}.max"), "1".to_string()]).collect();
        Job { function: "olib_batchStats", arguments: vec![], lines, stall: Duration::from_secs(60) }
    }

    /// A 3ds Max that writes its log at once and ends, or hangs.
    struct Fake {
        hangs: bool,
    }

    impl Process for Fake {
        fn ended(&mut self) -> bool {
            !self.hangs
        }
        fn kill(&mut self) {}
    }

    type Ran = (Vec<(usize, Outcome)>, Vec<String>, Result<(), String>);

    fn run_fake(job: &Job, logs: &[&str], hangs: bool) -> Ran {
        let dir = folder(&format!("run{}{}{hangs}", job.lines.len(), logs.len()));
        let (mut outcomes, mut jobs, mut launches) = (Vec::new(), Vec::new(), 0);
        let result = run_with(
            &dir,
            job,
            &|| false,
            &mut |n, o| outcomes.push((n, o)),
            &mut |script| {
                let dir = script.parent().unwrap();
                jobs.push(fs::read_to_string(dir.join("jobs.txt")).unwrap());
                fs::write(dir.join("log.txt"), logs.get(launches).copied().unwrap_or("")).unwrap();
                launches += 1;
                Ok(Box::new(Fake { hangs: hangs && launches == 1 }))
            },
            Duration::from_millis(1),
        );
        let _ = fs::remove_dir_all(&dir);
        (outcomes, jobs, result)
    }

    #[test]
    fn every_line_is_told_once() {
        let (outcomes, jobs, result) = run_fake(&job(2), &["\u{feff}ok\t1\t12\t8\n\u{feff}fail\t2\topen\n"], false);
        assert_eq!(outcomes, [(0, Ok(vec!["12".into(), "8".into()])), (1, Err("open".into()))]);
        assert_eq!(jobs, ["\u{feff}S:\\m1.max\t1\r\nS:\\m2.max\t1\r\n"]);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn a_file_that_closes_3ds_max_is_skipped_and_the_rest_goes_on() {
        let (outcomes, jobs, result) = run_fake(&job(3), &["ok\t1\ta\n", "ok\t1\tc\n"], false);
        assert_eq!(outcomes, [(0, Ok(vec!["a".into()])), (1, Err(STUCK.into())), (2, Ok(vec!["c".into()]))]);
        assert_eq!(jobs[1], "\u{feff}S:\\m3.max\t1\r\n");
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn a_3ds_max_that_never_works_is_not_started_for_every_file() {
        let (outcomes, jobs, result) = run_fake(&job(50), &[], false);
        assert_eq!((outcomes.len(), jobs.len()), (2, 2));
        assert!(result.is_err());
    }

    #[test]
    fn a_hung_file_is_given_up_after_the_stall() {
        let mut job = job(2);
        // the log moves with the first line, and then nothing
        job.stall = Duration::from_millis(20);
        let (outcomes, _, result) = run_fake(&job, &["ok\t1\ta\n"], true);
        assert_eq!(outcomes, [(0, Ok(vec!["a".into()])), (1, Err(STUCK.into()))]);
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn a_line_still_being_written_waits() {
        let dir = folder("partial");
        let log = dir.join("log.txt");
        fs::write(&log, "ok\t1\ta\nok\t2\tb").unwrap();
        assert_eq!(read_log(&log), [Ok(vec!["a".to_string()])]);
    }

    #[test]
    fn the_script_loads_both_files_and_calls_the_function() {
        let dir = folder("script");
        let mut job = job(1);
        (job.function, job.arguments) = ("olib_batchPreviews", vec![512.0, 1.15]);
        let (script, _) = prepare(&dir, &job, &[vec!["S:\\a\tb.max".into()]]).unwrap();
        let text = fs::read_to_string(&script).unwrap();
        assert!(text.starts_with("\u{feff}fileIn \""), "{text}");
        assert!(text.contains("olib_batch.ms\"") && text.trim_end().ends_with("log.txt\" 512 1.15"), "{text}");
        assert_eq!(fs::read_to_string(dir.join("jobs.txt")).unwrap(), "\u{feff}S:\\a b.max\r\n");
    }

    #[test]
    fn every_job_function_is_in_the_script_that_travels() {
        let text = String::from_utf8_lossy(BATCH);
        for name in ["olib_batchPreviews", "olib_batchStats", "olib_batchPaths", "olib_batchPrepare"] {
            assert!(text.contains(&format!("fn {name} ")), "{name}");
        }
        assert!(BATCH.starts_with(&[0xEF, 0xBB, 0xBF]), "3ds Max reads it as UTF-8 by its BOM");
        let startup = String::from_utf8_lossy(crate::bridge_install::STARTUP).into_owned();
        assert!(startup.contains(&format!("systemTools.getEnvVariable \"{VARIABLE}\"")), "the bridge stays off in the background");
    }

    #[test]
    fn the_newest_3ds_max_comes_first() {
        let vars = [("ADSK_3DSMAX_x64_2025", "S:\\M25"), ("ADSK_3DSMAX_x64_2026", "S:\\M26"), ("OTHER", "x")];
        let found = installs(vars.into_iter().map(|(a, b)| (a.to_string(), b.to_string())), &[]);
        assert_eq!(found, [(2026, PathBuf::from("S:\\M26")), (2025, PathBuf::from("S:\\M25"))]);
    }
}
