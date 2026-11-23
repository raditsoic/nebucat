use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use async_channel::{Receiver, Sender};
use nebucat_core::plugin_proto::{
    ActionResult, Item, Manifest, Response, action_request, parse_items, query_request,
};
use serde_json::Value;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

const REQUEST_TIMEOUT: Duration = Duration::from_millis(2000);
const STDERR_LINES: usize = 20;
// Grace period for a plugin that closed stdout to finish exiting before it is killed.
const EXIT_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub keyword: String,
    pub dir: PathBuf,
    pub manifest: Manifest,
}

pub struct Plugins {
    infos: Vec<PluginInfo>,
    procs: HashMap<String, Proc>,
}

impl Plugins {
    pub fn new() -> Plugins {
        Plugins { infos: Vec::new(), procs: HashMap::new() }
    }

    pub fn discover(&mut self, root: &Path) -> Vec<String> {
        let mut banners = Vec::new();
        let mut dirs: Vec<(String, PathBuf)> = match std::fs::read_dir(root) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
                .collect(),
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("plugins: can't read {}: {e}", root.display());
                }
                Vec::new()
            }
        };
        dirs.sort();

        let mut infos: Vec<PluginInfo> = Vec::new();
        for (dir_name, dir) in dirs {
            let manifest_path = dir.join("plugin.toml");
            if !manifest_path.is_file() {
                continue;
            }
            let text = match std::fs::read_to_string(&manifest_path) {
                Ok(t) => t,
                Err(e) => {
                    banners.push(format!("Plugin {dir_name}: {e}"));
                    continue;
                }
            };
            let manifest: Manifest = match toml::from_str(&text) {
                Ok(m) => m,
                Err(e) => {
                    banners.push(format!("Plugin {dir_name}: {}", toml_error(&text, &e)));
                    continue;
                }
            };
            let k = &manifest.keyword;
            if k.is_empty() || k.chars().any(char::is_whitespace) || k.starts_with(['>', '/', '!']) {
                banners.push(format!("Plugin {dir_name}: keyword \"{k}\" not allowed"));
                continue;
            }
            if let Some(other) = infos.iter().find(|i| i.keyword == *k) {
                let other_dir = other.dir.file_name().unwrap_or_default().to_string_lossy();
                banners.push(format!("Plugin {dir_name}: keyword \"{k}\" already used by {other_dir}"));
                continue;
            }
            infos.push(PluginInfo {
                id: manifest.id.clone(),
                name: manifest.name.clone(),
                keyword: manifest.keyword.clone(),
                dir,
                manifest,
            });
        }

        self.procs.retain(|keyword, _| {
            let old = self.infos.iter().find(|i| i.keyword == *keyword);
            let new = infos.iter().find(|i| i.keyword == *keyword);
            matches!((old, new), (Some(o), Some(n)) if o.dir == n.dir && o.manifest == n.manifest)
        });
        self.infos = infos;
        log::info!("plugins: {} loaded", self.infos.len());
        banners
    }

    pub fn keywords(&self) -> Vec<String> {
        self.infos.iter().map(|i| i.keyword.clone()).collect()
    }

    pub fn info(&self, keyword: &str) -> Option<&PluginInfo> {
        self.infos.iter().find(|i| i.keyword == keyword)
    }

    pub fn query(&mut self, keyword: &str, query: &str) -> Receiver<Result<Vec<Item>, String>> {
        let (tx, rx) = async_channel::bounded(1);
        self.send(keyword, Pending::Query(tx), |id| query_request(id, query));
        rx
    }

    pub fn action(&mut self, keyword: &str, data: &Value) -> Receiver<Result<ActionResult, String>> {
        let (tx, rx) = async_channel::bounded(1);
        self.send(keyword, Pending::Action(tx), |id| action_request(id, data));
        rx
    }

    pub fn icon_path(&self, keyword: &str, icon: &str) -> PathBuf {
        let path = Path::new(icon);
        match self.info(keyword) {
            Some(info) if path.is_relative() => info.dir.join(path),
            _ => path.to_path_buf(),
        }
    }

    fn send(&mut self, keyword: &str, pending: Pending, make_line: impl FnOnce(u64) -> String) {
        let Some(info) = self.infos.iter().find(|i| i.keyword == keyword) else {
            pending.fail(format!("No plugin uses keyword \"{keyword}\""));
            return;
        };
        let alive = self
            .procs
            .get(keyword)
            .is_some_and(|p| p.shared.lock().unwrap().dead.is_none());
        if !alive {
            self.procs.remove(keyword);
            match Proc::spawn(info) {
                Ok(p) => {
                    self.procs.insert(keyword.to_string(), p);
                }
                Err(e) => {
                    log::warn!("plugin {}: spawn failed: {e}", info.id);
                    pending.fail(format!("Couldn't start {}: {e}", info.name));
                    return;
                }
            }
        }
        if let Some(p) = self.procs.get_mut(keyword) {
            p.send(pending, make_line);
        }
    }
}

impl Drop for Plugins {
    fn drop(&mut self) {
        self.procs.clear();
    }
}

enum Pending {
    Query(Sender<Result<Vec<Item>, String>>),
    Action(Sender<Result<ActionResult, String>>),
}

impl Pending {
    fn fail(self, msg: String) {
        match self {
            Pending::Query(tx) => {
                let _ = tx.try_send(Err(msg));
            }
            Pending::Action(tx) => {
                let _ = tx.try_send(Err(msg));
            }
        }
    }

    fn complete(self, name: &str, resp: Response) {
        if let Some(err) = resp.error {
            return self.fail(format!("{name}: {}", err.message));
        }
        let result = resp.result.unwrap_or(Value::Null);
        match self {
            Pending::Query(tx) => {
                let _ = tx.try_send(parse_items(&result).map_err(|e| format!("{name}: {e}")));
            }
            Pending::Action(tx) => {
                let r = serde_json::from_value::<ActionResult>(result).map_err(|e| format!("{name}: {e}"));
                let _ = tx.try_send(r);
            }
        }
    }
}

struct Shared {
    pending: HashMap<u64, Pending>,
    // Set once stdout closes; holds the full "<name> stopped: ..." message.
    dead: Option<String>,
}

struct Proc {
    name: String,
    child: Arc<Mutex<Child>>,
    stdin: ChildStdin,
    shared: Arc<Mutex<Shared>>,
    deadlines: mpsc::Sender<(u64, Instant)>,
    next_id: u64,
    last_query: Option<u64>,
}

impl Proc {
    fn spawn(info: &PluginInfo) -> std::io::Result<Proc> {
        let Some((program, args)) = info.manifest.command.split_first() else {
            return Err(std::io::Error::other("command is empty"));
        };
        // Relative paths with a directory part resolve against the plugin folder, not nebucat's cwd.
        let program = Path::new(program);
        let program = if program.is_relative() && program.components().count() > 1 {
            info.dir.join(program)
        } else {
            program.to_path_buf()
        };
        let mut child = Command::new(program)
            .args(args)
            .current_dir(&info.dir)
            .creation_flags(CREATE_NO_WINDOW.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        log::info!("plugin {}: started pid {}", info.id, child.id());

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");
        let child = Arc::new(Mutex::new(child));
        let shared = Arc::new(Mutex::new(Shared { pending: HashMap::new(), dead: None }));
        let ring = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_LINES)));

        let (stderr_done_tx, stderr_done_rx) = mpsc::channel::<()>();
        {
            let ring = ring.clone();
            let id = info.id.clone();
            thread::spawn(move || {
                let _done = stderr_done_tx;
                read_lines(stderr, |line| {
                    log::debug!("plugin {id} stderr: {line}");
                    let mut ring = ring.lock().unwrap();
                    if ring.len() == STDERR_LINES {
                        ring.pop_front();
                    }
                    ring.push_back(line);
                });
            });
        }

        {
            let child = child.clone();
            let shared = shared.clone();
            let name = info.name.clone();
            let id = info.id.clone();
            thread::spawn(move || {
                read_lines(stdout, |line| match serde_json::from_str::<Response>(&line) {
                    Ok(resp) => {
                        let pending = shared.lock().unwrap().pending.remove(&resp.id);
                        if let Some(p) = pending {
                            p.complete(&name, resp);
                        }
                    }
                    Err(_) => log::debug!("plugin {id}: ignored stdout line: {line}"),
                });

                let status = wait_exit(&child);
                let _ = stderr_done_rx.recv_timeout(Duration::from_millis(200));
                let last_err = ring.lock().unwrap().iter().rev().find(|l| !l.trim().is_empty()).cloned();
                let detail = match (last_err, status) {
                    (Some(line), _) => line.trim().to_string(),
                    (None, Some(code)) => format!("exit {code}"),
                    (None, None) => "exit unknown".to_string(),
                };
                log::warn!("plugin {id}: stopped: {detail}");
                let msg = format!("{name} stopped: {detail}");
                let drained: Vec<Pending> = {
                    let mut s = shared.lock().unwrap();
                    s.dead = Some(msg.clone());
                    s.pending.drain().map(|(_, p)| p).collect()
                };
                for p in drained {
                    p.fail(msg.clone());
                }
            });
        }

        let (deadlines, deadline_rx) = mpsc::channel::<(u64, Instant)>();
        {
            let shared = shared.clone();
            let name = info.name.clone();
            // Every request has the same timeout, so deadlines arrive in order.
            thread::spawn(move || {
                for (id, deadline) in deadline_rx {
                    let now = Instant::now();
                    if deadline > now {
                        thread::sleep(deadline - now);
                    }
                    let pending = shared.lock().unwrap().pending.remove(&id);
                    if let Some(p) = pending {
                        p.fail(format!("{name} didn't answer in 2 s."));
                    }
                }
            });
        }

        Ok(Proc {
            name: info.name.clone(),
            child,
            stdin,
            shared,
            deadlines,
            next_id: 0,
            last_query: None,
        })
    }

    fn send(&mut self, pending: Pending, make_line: impl FnOnce(u64) -> String) {
        self.next_id += 1;
        let id = self.next_id;
        {
            let mut s = self.shared.lock().unwrap();
            if let Some(msg) = &s.dead {
                pending.fail(msg.clone());
                return;
            }
            // Only the latest query matters; dropping the sender closes the stale receiver.
            if matches!(pending, Pending::Query(_))
                && let Some(prev) = self.last_query.replace(id)
            {
                s.pending.remove(&prev);
            }
            s.pending.insert(id, pending);
        }
        let _ = self.deadlines.send((id, Instant::now() + REQUEST_TIMEOUT));

        let mut line = make_line(id);
        line.push('\n');
        if let Err(e) = self.stdin.write_all(line.as_bytes()).and_then(|_| self.stdin.flush()) {
            log::warn!("plugin {}: write failed: {e}", self.name);
            // Killing closes stdout, so the reader thread reports the stop to every pending request.
            let _ = self.child.lock().unwrap().kill();
        }
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        let mut child = self.child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn read_lines(stream: impl Read, mut on_line: impl FnMut(String)) {
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                on_line(line.trim_end_matches(['\r', '\n']).to_string());
            }
        }
    }
}

fn wait_exit(child: &Mutex<Child>) -> Option<i32> {
    let deadline = Instant::now() + EXIT_GRACE;
    loop {
        {
            let mut c = child.lock().unwrap();
            match c.try_wait() {
                Ok(Some(status)) => return status.code(),
                Ok(None) if Instant::now() < deadline => {}
                _ => {
                    let _ = c.kill();
                    return c.wait().ok().and_then(|s| s.code());
                }
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn toml_error(text: &str, e: &toml::de::Error) -> String {
    let msg = e.message().trim();
    match e.span() {
        Some(span) => {
            let end = span.start.min(text.len());
            let line = text.as_bytes()[..end].iter().filter(|&&b| b == b'\n').count() + 1;
            format!("line {line}: {msg}")
        }
        None => msg.to_string(),
    }
}
