use std::io::Read;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use nebucat_core::config::Shell;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
    TerminateJobObject,
};
use windows::core::PCWSTR;

use crate::shellexec::shell_execute;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_LINES: usize = 5000;
const MAX_BYTES: usize = 2 * 1024 * 1024;
const TRUNCATED_LINE: &[u8] = b"Output truncated after 5000 lines.\n";
const POLL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Running,
    Exited { code: i32, secs: f64 },
    TimedOut { secs: u64 },
    Cancelled,
    Failed(String),
}

pub struct Snapshot {
    pub status: Status,
    pub output: Vec<u8>,
    pub stdout: Vec<u8>,
    pub truncated: bool,
}

struct State {
    status: Status,
    output: Vec<u8>,
    stdout: Vec<u8>,
    lines: usize,
    truncated: bool,
}

impl State {
    fn append(&mut self, chunk: &[u8], is_stdout: bool) {
        if self.truncated {
            return;
        }
        let mut take = chunk.len().min(MAX_BYTES - self.output.len());
        if self.lines >= MAX_LINES {
            take = 0;
        } else if let Some(pos) = chunk[..take]
            .iter()
            .enumerate()
            .filter(|(_, b)| **b == b'\n')
            .nth(MAX_LINES - 1 - self.lines)
            .map(|(i, _)| i)
        {
            take = pos + 1;
        }
        let kept = &chunk[..take];
        self.lines += kept.iter().filter(|b| **b == b'\n').count();
        self.output.extend_from_slice(kept);
        if is_stdout {
            self.stdout.extend_from_slice(kept);
        }
        if take < chunk.len() {
            self.truncated = true;
            if self.output.last().is_some_and(|b| *b != b'\n') {
                self.output.push(b'\n');
            }
            self.output.extend_from_slice(TRUNCATED_LINE);
        }
    }
}

struct Job(HANDLE);

// Job object handles are process-wide kernel handles, usable from any thread.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    fn new() -> windows::core::Result<Job> {
        let handle = unsafe { CreateJobObjectW(None, PCWSTR::null())? };
        let job = Job(handle);
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
        }
        Ok(job)
    }

    fn assign(&self, child: &Child) -> windows::core::Result<()> {
        unsafe { AssignProcessToJobObject(self.0, HANDLE(child.as_raw_handle())) }
    }

    fn terminate(&self) {
        if let Err(e) = unsafe { TerminateJobObject(self.0, 1) } {
            log::warn!("TerminateJobObject failed: {e}");
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub struct CommandRun {
    state: Arc<Mutex<State>>,
    job: Option<Arc<Job>>,
    started: Instant,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

impl CommandRun {
    pub fn start(cmd: &str, shell: Shell, timeout_secs: u64) -> CommandRun {
        let started = Instant::now();
        let state = Arc::new(Mutex::new(State {
            status: Status::Running,
            output: Vec::new(),
            stdout: Vec::new(),
            lines: 0,
            truncated: false,
        }));
        let failed = |msg: String| {
            lock(&state).status = Status::Failed(msg);
            CommandRun { state: state.clone(), job: None, started }
        };

        let mut command = match shell {
            Shell::Cmd => {
                let mut c = Command::new("cmd.exe");
                c.raw_arg(format!("/d /s /c \"chcp 65001>nul & {cmd}\""));
                c
            }
            Shell::Powershell => {
                let mut c = Command::new("powershell.exe");
                c.raw_arg(format!(
                    "-NoProfile -NonInteractive -Command \"[Console]::OutputEncoding=[Text.Encoding]::UTF8; {cmd}\""
                ));
                c
            }
        };
        command
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(home) = std::env::var_os("USERPROFILE") {
            command.current_dir(home);
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => return failed(e.to_string()),
        };
        let job = match Job::new().and_then(|job| job.assign(&child).map(|()| job)) {
            Ok(job) => Arc::new(job),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return failed(format!("couldn't create a job object: {e}"));
            }
        };

        let mut readers: Vec<JoinHandle<()>> = Vec::with_capacity(2);
        if let Some(out) = child.stdout.take() {
            readers.push(spawn_reader(out, true, state.clone()));
        }
        if let Some(err) = child.stderr.take() {
            readers.push(spawn_reader(err, false, state.clone()));
        }

        let waiter_state = state.clone();
        let waiter_job = job.clone();
        std::thread::spawn(move || {
            wait_loop(child, readers, waiter_state, waiter_job, started, timeout_secs)
        });

        CommandRun { state, job: Some(job), started }
    }

    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn snapshot(&self) -> Snapshot {
        let s = lock(&self.state);
        Snapshot {
            status: s.status.clone(),
            output: s.output.clone(),
            stdout: s.stdout.clone(),
            truncated: s.truncated,
        }
    }

    pub fn cancel(&self) {
        let mut s = lock(&self.state);
        if s.status == Status::Running {
            if let Some(job) = &self.job {
                job.terminate();
            }
            s.status = Status::Cancelled;
        }
    }
}

impl Drop for CommandRun {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn spawn_reader(mut pipe: impl Read + Send + 'static, is_stdout: bool, state: Arc<Mutex<State>>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => lock(&state).append(&buf[..n], is_stdout),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    })
}

fn wait_loop(
    mut child: Child,
    readers: Vec<JoinHandle<()>>,
    state: Arc<Mutex<State>>,
    job: Arc<Job>,
    started: Instant,
    timeout_secs: u64,
) {
    let deadline = started + Duration::from_secs(timeout_secs);
    let mut code = None;
    // Readers can outlive the shell when a grandchild holds the pipes, so the deadline still applies.
    loop {
        if code.is_none() {
            match child.try_wait() {
                Ok(Some(exit)) => code = Some(exit.code().unwrap_or(-1)),
                Ok(None) => {}
                Err(e) => {
                    log::warn!("try_wait failed: {e}");
                    code = Some(-1);
                }
            }
        }
        if code.is_some() && readers.iter().all(|r| r.is_finished()) {
            break;
        }
        if Instant::now() >= deadline {
            let mut s = lock(&state);
            if s.status == Status::Running {
                job.terminate();
                s.status = Status::TimedOut { secs: timeout_secs };
            }
        }
        if lock(&state).status != Status::Running {
            let _ = child.wait();
            break;
        }
        std::thread::sleep(POLL);
    }
    for reader in readers {
        let _ = reader.join();
    }
    let mut s = lock(&state);
    if s.status == Status::Running {
        s.status = Status::Exited {
            code: code.unwrap_or(-1),
            secs: started.elapsed().as_secs_f64(),
        };
    }
}

pub fn open_in_terminal(cmd: &str) -> Result<(), i32> {
    shell_execute("open", "wt.exe", Some(&format!("new-tab cmd /k {cmd}")))
        .or_else(|_| shell_execute("open", "cmd.exe", Some(&format!("/k {cmd}"))))
}
