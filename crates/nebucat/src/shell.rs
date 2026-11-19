use std::sync::{Arc, Mutex, mpsc};

use async_channel::Sender;
use nebucat_core::hotkey::Hotkey;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use windows::Win32::Foundation::{HANDLE, LPARAM, LRESULT, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::Threading::{GetCurrentThreadId, INFINITE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, MOD_ALT,
    MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, SendInput, UnregisterHotKey,
    VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, HHOOK, KBDLLHOOKSTRUCT, MSG, MsgWaitForMultipleObjects,
    PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, QS_ALLINPUT, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_APP, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

const HOTKEY_ID: i32 = 1;
const WM_REREGISTER: u32 = WM_APP + 1;
const WM_QUIT_SHELL: u32 = WM_APP + 2;
const WM_MODIFIER_TAP: u32 = WM_APP + 3;

const MOD_CTRL_ID: u8 = 1;
const MOD_ALT_ID: u8 = 2;
const MOD_SHIFT_ID: u8 = 3;
const MOD_WIN_ID: u8 = 4;
// Unassigned virtual key; pressing it between Win/Alt down and up stops Start/menu-bar activation.
const VK_DUMMY: u16 = 0xE8;
const LLKHF_INJECTED: u32 = 0x10;

// The hook proc is a bare fn, so its state lives in statics. Only the shell thread writes TARGET.
static TARGET: AtomicU8 = AtomicU8::new(0);
static ARMED: AtomicBool = AtomicBool::new(false);
static CLEAN: AtomicBool = AtomicBool::new(false);
static SHELL_THREAD: AtomicU32 = AtomicU32::new(0);

fn modifier_id(vk: u32) -> u8 {
    match vk {
        0xA2 | 0xA3 => MOD_CTRL_ID,
        0xA4 | 0xA5 => MOD_ALT_ID,
        0xA0 | 0xA1 => MOD_SHIFT_ID,
        0x5B | 0x5C => MOD_WIN_ID,
        _ => 0,
    }
}

fn send_dummy_key() {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(VK_DUMMY),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [key(Default::default()), key(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

// Fires on release of the target modifier when no other key was pressed while it was held.
unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == 0 {
        let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if kb.flags.0 & LLKHF_INJECTED == 0 {
            let msg = wparam.0 as u32;
            let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
            let target = TARGET.load(Ordering::Relaxed);
            if target != 0 && modifier_id(kb.vkCode) == target {
                if down {
                    if !ARMED.swap(true, Ordering::Relaxed) {
                        CLEAN.store(true, Ordering::Relaxed);
                    }
                } else if up && ARMED.swap(false, Ordering::Relaxed) && CLEAN.load(Ordering::Relaxed) {
                    if target == MOD_WIN_ID || target == MOD_ALT_ID {
                        send_dummy_key();
                    }
                    unsafe {
                        let _ = PostThreadMessageW(
                            SHELL_THREAD.load(Ordering::Relaxed),
                            WM_MODIFIER_TAP,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    }
                }
            } else if down {
                CLEAN.store(false, Ordering::Relaxed);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

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

fn release(hook: &mut Option<HHOOK>) {
    TARGET.store(0, Ordering::Relaxed);
    ARMED.store(false, Ordering::Relaxed);
    if let Some(h) = hook.take() {
        unsafe {
            let _ = UnhookWindowsHookEx(h);
        }
    }
    unsafe {
        let _ = UnregisterHotKey(None, HOTKEY_ID);
    }
}

fn register(pending: &Mutex<(Hotkey, String)>, tx: &Sender<ShellEvent>, hook: &mut Option<HHOOK>) {
    release(hook);
    let (hk, label) = pending.lock().unwrap().clone();
    let ev = if hk.is_modifier_only() {
        let id = if hk.ctrl {
            MOD_CTRL_ID
        } else if hk.alt {
            MOD_ALT_ID
        } else if hk.shift {
            MOD_SHIFT_ID
        } else {
            MOD_WIN_ID
        };
        TARGET.store(id, Ordering::Relaxed);
        match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0) } {
            Ok(h) => {
                *hook = Some(h);
                ShellEvent::HotkeyOk
            }
            Err(e) => {
                TARGET.store(0, Ordering::Relaxed);
                log::warn!("SetWindowsHookExW {label}: {e}");
                ShellEvent::HotkeyError(format!(
                    "{label} could not be hooked. Change hotkey in config.toml."
                ))
            }
        }
    } else {
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
        match unsafe { RegisterHotKey(None, HOTKEY_ID, HOT_KEY_MODIFIERS(mods.0), hk.vk) } {
            Ok(()) => ShellEvent::HotkeyOk,
            Err(e) => {
                log::warn!("RegisterHotKey {label}: {e}");
                ShellEvent::HotkeyError(format!(
                    "{label} is taken by another app. Change hotkey in config.toml."
                ))
            }
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
    SHELL_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);

    let mut hook: Option<HHOOK> = None;
    register(&pending, &tx, &mut hook);

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
                WM_HOTKEY | WM_MODIFIER_TAP => {
                    let _ = tx.send_blocking(ShellEvent::Toggle);
                }
                WM_REREGISTER => register(&pending, &tx, &mut hook),
                WM_QUIT_SHELL => {
                    release(&mut hook);
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
