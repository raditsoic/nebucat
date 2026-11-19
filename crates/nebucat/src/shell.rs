use std::sync::{Arc, Mutex, mpsc};

use async_channel::Sender;
use nebucat_core::hotkey::Hotkey;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use windows::Win32::Foundation::{HANDLE, LPARAM, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::Threading::{GetCurrentThreadId, INFINITE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MsgWaitForMultipleObjects, PM_NOREMOVE, PM_REMOVE, PeekMessageW,
    PostThreadMessageW, QS_ALLINPUT, TranslateMessage, WM_APP, WM_HOTKEY,
};

const HOTKEY_ID: i32 = 1;
const WM_REREGISTER: u32 = WM_APP + 1;
const WM_QUIT_SHELL: u32 = WM_APP + 2;

#[derive(Debug, Clone)]
pub enum ShellEvent {
    Toggle,
    Show,
    ReloadIndex,
    OpenConfigFolder,
    Quit,
    HotkeyOk,
    HotkeyError(String),
}

pub struct ShellHandle {
    thread_id: u32,
    pending_hotkey: Arc<Mutex<(Hotkey, String)>>,
}

impl ShellHandle {
    pub fn set_hotkey(&self, hk: Hotkey, label: String) {
        *self.pending_hotkey.lock().unwrap() = (hk, label);
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_REREGISTER, WPARAM(0), LPARAM(0));
        }
    }
}

impl Drop for ShellHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT_SHELL, WPARAM(0), LPARAM(0));
        }
    }
}

/// `show_event` must stay valid for the process lifetime.
pub fn start(hotkey: Hotkey, label: String, show_event: HANDLE, tx: Sender<ShellEvent>) -> ShellHandle {
    let pending = Arc::new(Mutex::new((hotkey, label)));
    let (id_tx, id_rx) = mpsc::channel();
    let pending_thread = pending.clone();
    let show_event = show_event.0 as usize;
    std::thread::Builder::new()
        .name("shell".into())
        .spawn(move || run(pending_thread, HANDLE(show_event as _), tx, id_tx))
        .expect("spawn shell thread");
    let thread_id = id_rx.recv().expect("shell thread id");
    ShellHandle { thread_id, pending_hotkey: pending }
}

fn register(pending: &Mutex<(Hotkey, String)>, tx: &Sender<ShellEvent>) {
    let (hk, label) = pending.lock().unwrap().clone();
    let mut mods = MOD_NOREPEAT;
    if hk.ctrl {
        mods |= MOD_CONTROL;
    }
    if hk.alt {
        mods |= MOD_ALT;
    }
    if hk.shift {
        mods |= MOD_SHIFT;
    }
    if hk.win {
        mods |= MOD_WIN;
    }
    let ok = unsafe { RegisterHotKey(None, HOTKEY_ID, HOT_KEY_MODIFIERS(mods.0), hk.vk) };
    let ev = match ok {
        Ok(()) => ShellEvent::HotkeyOk,
        Err(e) => {
            log::warn!("RegisterHotKey {label}: {e}");
            ShellEvent::HotkeyError(format!(
                "{label} is taken by another app. Change hotkey in config.toml."
            ))
        }
    };
    let _ = tx.send_blocking(ev);
}

fn run(
    pending: Arc<Mutex<(Hotkey, String)>>,
    show_event: HANDLE,
    tx: Sender<ShellEvent>,
    id_tx: mpsc::Sender<u32>,
) {
    let mut msg = MSG::default();
    // Forces creation of this thread's message queue before PostThreadMessageW can target it.
    unsafe {
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
    }
    let _ = id_tx.send(unsafe { GetCurrentThreadId() });

    register(&pending, &tx);

    let menu = Menu::new();
    let show = MenuItem::new("Show nebucat", true, None);
    let config = MenuItem::new("Open config folder", true, None);
    let reindex = MenuItem::new("Rebuild app index", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let _ = menu.append_items(&[&show, &config, &reindex, &quit]);
    let ids = [
        (show.id().clone(), ShellEvent::Show),
        (config.id().clone(), ShellEvent::OpenConfigFolder),
        (reindex.id().clone(), ShellEvent::ReloadIndex),
        (quit.id().clone(), ShellEvent::Quit),
    ];
    let menu_tx = tx.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if let Some((_, ev)) = ids.iter().find(|(id, _)| *id == e.id) {
            let _ = menu_tx.send_blocking(ev.clone());
        }
    }));
    let tray_tx = tx.clone();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = e
        {
            let _ = tray_tx.send_blocking(ShellEvent::Show);
        }
    }));
    let side = crate::logo::SIZE as u32;
    let tray = Icon::from_rgba(crate::logo::RGBA.to_vec(), side, side)
        .map_err(|e| e.to_string())
        .and_then(|icon| {
            TrayIconBuilder::new()
                .with_tooltip("nebucat")
                .with_icon(icon)
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()
                .map_err(|e| e.to_string())
        });
    let _tray = match tray {
        Ok(t) => Some(t),
        Err(e) => {
            log::error!("tray icon: {e}");
            None
        }
    };

    loop {
        let r = unsafe { MsgWaitForMultipleObjects(Some(&[show_event]), false, INFINITE, QS_ALLINPUT) };
        if r == WAIT_OBJECT_0 {
            let _ = tx.send_blocking(ShellEvent::Show);
            continue;
        }
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            match msg.message {
                WM_HOTKEY => {
                    let _ = tx.send_blocking(ShellEvent::Toggle);
                }
                WM_REREGISTER => {
                    unsafe {
                        let _ = UnregisterHotKey(None, HOTKEY_ID);
                    }
                    register(&pending, &tx);
                }
                WM_QUIT_SHELL => {
                    unsafe {
                        let _ = UnregisterHotKey(None, HOTKEY_ID);
                    }
                    return;
                }
                _ => unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                },
            }
        }
    }
}
