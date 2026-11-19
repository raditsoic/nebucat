use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::RenderImage;
use nebucat_core::history::History;
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, KF_FLAG_DEFAULT,
    SHGetKnownFolderItem, SIGDN, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
};

use crate::icons;
use crate::paths::config_dir;

const ICON_SIZE: i32 = 48;

#[derive(Clone)]
pub struct AppEntry {
    pub name: String,
    pub id: String,
    pub icon: Option<Arc<RenderImage>>,
}

/// `reuse` maps app ids to icons from the previous index; those apps skip icon extraction.
pub fn spawn_index(
    reuse: HashMap<String, Arc<RenderImage>>,
) -> async_channel::Receiver<Result<Vec<AppEntry>, String>> {
    let (tx, rx) = async_channel::bounded(1);
    let thread_tx = tx.clone();
    let spawned = std::thread::Builder::new()
        .name("app-index".into())
        .spawn(move || {
            let started = Instant::now();
            let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                .ok()
                .and_then(|()| {
                    let apps = enumerate(&reuse);
                    unsafe { CoUninitialize() };
                    apps
                })
                .map_err(|e| e.message());
            match &result {
                Ok(apps) => log::info!(
                    "indexed {} apps in {} ms",
                    apps.len(),
                    started.elapsed().as_millis()
                ),
                Err(e) => log::error!("app index failed: {e}"),
            }
            let _ = thread_tx.send_blocking(result);
        });
    if let Err(e) = spawned {
        log::error!("app index thread failed to start: {e}");
        let _ = tx.try_send(Err(e.to_string()));
    }
    rx
}

fn enumerate(reuse: &HashMap<String, Arc<RenderImage>>) -> windows::core::Result<Vec<AppEntry>> {
    let folder: IShellItem =
        unsafe { SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) }?;
    let items: IEnumShellItems = unsafe { folder.BindToHandler(None, &BHID_EnumItems) }?;
    let mut apps = Vec::new();
    loop {
        let mut slot: [Option<IShellItem>; 1] = [None];
        let mut fetched = 0u32;
        // S_FALSE with zero fetched marks the end; it maps to Ok.
        unsafe { items.Next(&mut slot, Some(&raw mut fetched)) }?;
        let Some(item) = slot[0].take() else { break };
        if fetched == 0 {
            break;
        }
        let (Some(name), Some(id)) = (
            display_name(&item, SIGDN_NORMALDISPLAY),
            display_name(&item, SIGDN_PARENTRELATIVEPARSING),
        ) else {
            continue;
        };
        if name.is_empty() || id.is_empty() {
            continue;
        }
        let icon = match reuse.get(&id) {
            Some(icon) => Some(icon.clone()),
            None => icons::from_shell_item(&item, ICON_SIZE),
        };
        apps.push(AppEntry { name, id, icon });
    }
    apps.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    apps.dedup_by(|a, b| a.name == b.name && a.id == b.id);
    Ok(apps)
}

fn display_name(item: &IShellItem, sigdn: SIGDN) -> Option<String> {
    let raw = unsafe { item.GetDisplayName(sigdn) }.ok()?;
    let text = unsafe { raw.to_string() };
    unsafe { CoTaskMemFree(Some(raw.0 as *const _)) };
    text.ok()
}

pub fn launch(id: &str, admin: bool) -> Result<(), i32> {
    crate::shellexec::shell_execute(
        if admin { "runas" } else { "open" },
        &format!("shell:AppsFolder\\{id}"),
        None,
    )
}

pub fn load_history() -> History {
    let path = config_dir().join("history.json");
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            log::warn!("{} is corrupt, starting empty: {e}", path.display());
            History::default()
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => History::default(),
        Err(e) => {
            log::warn!("can't read {}, starting empty: {e}", path.display());
            History::default()
        }
    }
}

pub fn save_history(h: &History) {
    let dir = config_dir();
    let path = dir.join("history.json");
    let tmp = dir.join("history.json.tmp");
    let result = serde_json::to_vec(h).map_err(io::Error::other).and_then(|bytes| {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &path)
    });
    if let Err(e) = result {
        log::error!("saving {} failed: {e}", path.display());
    }
}
