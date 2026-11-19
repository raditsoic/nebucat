use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use libloading::Library;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};

const REQUEST_FILE_NAME: u32 = 0x1;
const REQUEST_PATH: u32 = 0x2;
const ERROR_IPC: u32 = 2;
const PATH_BUF_LEN: usize = 32768;
const ICON_SIZE: i32 = 48;

#[derive(Clone)]
pub struct FileHit {
    pub path: PathBuf,
    pub name: String,
    pub parent: String,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub icon: Option<Arc<gpui_kit::RenderImage>>,
}

#[derive(Debug, Clone)]
pub enum FilesError {
    DllMissing,
    NotRunning,
    Other(String),
}

type Reply = async_channel::Sender<Result<Vec<FileHit>, FilesError>>;

struct Req {
    query: String,
    max: u32,
    reply: Reply,
}

pub struct Files {
    tx: async_channel::Sender<Req>,
}

impl Files {
    pub fn start() -> Files {
        let (tx, rx) = async_channel::unbounded::<Req>();
        std::thread::Builder::new()
            .name("everything".into())
            .spawn(move || run(rx))
            .expect("spawn everything thread");
        Files { tx }
    }

    pub fn search(&self, query: String, max: u32) -> async_channel::Receiver<Result<Vec<FileHit>, FilesError>> {
        let (reply, rx) = async_channel::bounded(1);
        if self.tx.send_blocking(Req { query, max, reply }).is_err() {
            log::error!("everything thread is gone");
        }
        rx
    }
}

struct Everything {
    set_search: unsafe extern "system" fn(*const u16),
    set_max: unsafe extern "system" fn(u32),
    set_request_flags: unsafe extern "system" fn(u32),
    query: unsafe extern "system" fn(i32) -> i32,
    get_last_error: unsafe extern "system" fn() -> u32,
    get_num_results: unsafe extern "system" fn() -> u32,
    get_full_path: unsafe extern "system" fn(u32, *mut u16, u32) -> u32,
    // Keeps the fn pointers above valid.
    _lib: Library,
}

enum LoadError {
    Missing,
    Symbol(String),
}

unsafe fn sym<T: Copy>(lib: &Library, name: &[u8]) -> Result<T, LoadError> {
    unsafe { lib.get::<T>(name) }
        .map(|s| *s)
        .map_err(|e| LoadError::Symbol(format!("Everything64.dll: {e}")))
}

impl Everything {
    fn load() -> Result<Everything, LoadError> {
        let dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .ok_or(LoadError::Missing)?;
        let path = dir.join("Everything64.dll");
        let lib = unsafe { Library::new(&path) }.map_err(|e| {
            log::warn!("loading {}: {e}", path.display());
            LoadError::Missing
        })?;
        // SAFETY: signatures match the Everything SDK exports.
        unsafe {
            Ok(Everything {
                set_search: sym(&lib, b"Everything_SetSearchW\0")?,
                set_max: sym(&lib, b"Everything_SetMax\0")?,
                set_request_flags: sym(&lib, b"Everything_SetRequestFlags\0")?,
                query: sym(&lib, b"Everything_QueryW\0")?,
                get_last_error: sym(&lib, b"Everything_GetLastError\0")?,
                get_num_results: sym(&lib, b"Everything_GetNumResults\0")?,
                get_full_path: sym(&lib, b"Everything_GetResultFullPathNameW\0")?,
                _lib: lib,
            })
        }
    }

    fn search(&self, query: &str, max: u32, buf: &mut [u16]) -> Result<Vec<FileHit>, FilesError> {
        let wide: Vec<u16> = query.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            (self.set_search)(wide.as_ptr());
            (self.set_request_flags)(REQUEST_FILE_NAME | REQUEST_PATH);
            (self.set_max)(max);
            if (self.query)(1) == 0 {
                let code = (self.get_last_error)();
                return Err(if code == ERROR_IPC {
                    FilesError::NotRunning
                } else {
                    FilesError::Other(format!("Everything error {code}"))
                });
            }
            let count = (self.get_num_results)();
            let mut hits = Vec::with_capacity(count as usize);
            for i in 0..count {
                let len = (self.get_full_path)(i, buf.as_mut_ptr(), buf.len() as u32) as usize;
                if len == 0 {
                    continue;
                }
                let path = PathBuf::from(String::from_utf16_lossy(&buf[..len.min(buf.len())]));
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                let parent = path
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let icon = crate::icons::for_path(&path, ICON_SIZE);
                let meta = std::fs::metadata(&path).ok();
                let size = meta.as_ref().map(|m| m.len());
                let modified = meta.and_then(|m| m.modified().ok());
                hits.push(FileHit {
                    path,
                    name,
                    parent,
                    icon,
                    size,
                    modified,
                });
            }
            Ok(hits)
        }
    }
}

fn run(rx: async_channel::Receiver<Req>) {
    // STA for IShellItemImageFactory icons.
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if hr.is_err() {
        log::warn!("everything thread CoInitializeEx: {hr:?}");
    }
    let mut everything: Option<Everything> = None;
    let mut buf = vec![0u16; PATH_BUF_LEN];
    while let Ok(mut req) = rx.recv_blocking() {
        // Only the newest queued request matters; stale replies close unanswered.
        while let Ok(newer) = rx.try_recv() {
            req = newer;
        }
        if everything.is_none() {
            match Everything::load() {
                Ok(e) => everything = Some(e),
                Err(LoadError::Missing) => {
                    let _ = req.reply.try_send(Err(FilesError::DllMissing));
                    continue;
                }
                Err(LoadError::Symbol(msg)) => {
                    let _ = req.reply.try_send(Err(FilesError::Other(msg)));
                    continue;
                }
            }
        }
        let Some(e) = everything.as_ref() else { continue };
        let result = e.search(&req.query, req.max, &mut buf);
        let _ = req.reply.try_send(result);
    }
}
