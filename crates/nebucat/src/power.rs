use nebucat_core::power::PowerAction;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_NOT_ALL_ASSIGNED, GetLastError, HANDLE, LPARAM, LUID, WPARAM,
};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Power::{
    GetPwrCapabilities, IsPwrHibernateAllowed, IsPwrSuspendAllowed, SYSTEM_POWER_CAPABILITIES,
    SetSuspendState,
};
use windows::Win32::System::Shutdown::{
    InitiateShutdownW, SHTDN_REASON_FLAG_PLANNED, SHTDN_REASON_MAJOR_OTHER,
    SHTDN_REASON_MINOR_OTHER, SHUTDOWN_FLAGS, SHUTDOWN_HYBRID, SHUTDOWN_POWEROFF,
    SHUTDOWN_RESTART,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SC_MONITORPOWER, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SYSCOMMAND,
};
use windows::core::PCWSTR;

pub struct Caps {
    pub sleep: bool,
    pub hibernate: bool,
}

/// Cheap syscalls; called on every `!` refresh so `powercfg` changes show up live.
pub fn capabilities() -> Caps {
    Caps {
        sleep: modern_standby() || unsafe { IsPwrSuspendAllowed() },
        hibernate: unsafe { IsPwrHibernateAllowed() },
    }
}

/// Runs the action on a `power` thread; one reply. `Err` holds a Win32 error code.
pub fn run(action: PowerAction) -> async_channel::Receiver<Result<(), u32>> {
    let (tx, rx) = async_channel::bounded(1);
    let thread_tx = tx.clone();
    let spawned = std::thread::Builder::new()
        .name("power".into())
        .spawn(move || {
            log::info!("power: {action:?}");
            let result = execute(action);
            if let Err(code) = result {
                log::error!("power {action:?} failed: error {code}");
            }
            let _ = thread_tx.send_blocking(result);
        });
    if let Err(e) = spawned {
        log::error!("power thread failed to start: {e}");
        let _ = tx.try_send(Err(0));
    }
    rx
}

/// SetSuspendState blocks until the PC wakes, so this never runs on the gpui thread.
fn execute(action: PowerAction) -> Result<(), u32> {
    match action {
        PowerAction::Restart => {
            enable_shutdown_privilege()?;
            initiate(SHUTDOWN_RESTART)
        }
        PowerAction::Shutdown => {
            enable_shutdown_privilege()?;
            // Hybrid is Fast Startup; it fails when Fast Startup or hibernation is off.
            initiate(SHUTDOWN_POWEROFF | SHUTDOWN_HYBRID).or_else(|_| initiate(SHUTDOWN_POWEROFF))
        }
        PowerAction::Sleep if modern_standby() => {
            // SetSuspendState cannot enter Modern Standby; it hibernates or does nothing,
            // so turn the display off like the Start menu does.
            unsafe {
                SendMessageTimeoutW(
                    HWND_BROADCAST,
                    WM_SYSCOMMAND,
                    WPARAM(SC_MONITORPOWER as usize),
                    LPARAM(2),
                    SMTO_ABORTIFHUNG,
                    2000,
                    None,
                )
            };
            Ok(())
        }
        PowerAction::Sleep => {
            enable_shutdown_privilege()?;
            suspend(false)
        }
        PowerAction::Hibernate => {
            enable_shutdown_privilege()?;
            suspend(true)
        }
    }
}

fn modern_standby() -> bool {
    let mut caps = SYSTEM_POWER_CAPABILITIES::default();
    let ok = unsafe { GetPwrCapabilities(&mut caps) };
    ok && caps.AoAc
}

fn suspend(hibernate: bool) -> Result<(), u32> {
    if unsafe { SetSuspendState(hibernate, false, false) } {
        Ok(())
    } else {
        Err(unsafe { GetLastError() }.0)
    }
}

fn initiate(flags: SHUTDOWN_FLAGS) -> Result<(), u32> {
    // No SHUTDOWN_FORCE_OTHERS: apps with unsaved work must be able to block, like the Start menu.
    let code = unsafe {
        InitiateShutdownW(
            PCWSTR::null(),
            PCWSTR::null(),
            0,
            flags,
            SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_MINOR_OTHER | SHTDN_REASON_FLAG_PLANNED,
        )
    };
    if code == 0 { Ok(()) } else { Err(code) }
}

fn enable_shutdown_privilege() -> Result<(), u32> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) }
        .map_err(|e| win32_code(&e))?;
    let result = adjust(token);
    let _ = unsafe { CloseHandle(token) };
    result
}

fn adjust(token: HANDLE) -> Result<(), u32> {
    let mut luid = LUID::default();
    unsafe { LookupPrivilegeValueW(PCWSTR::null(), SE_SHUTDOWN_NAME, &mut luid) }
        .map_err(|e| win32_code(&e))?;
    let privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
    };
    unsafe { AdjustTokenPrivileges(token, false, Some(&privileges), 0, None, None) }
        .map_err(|e| win32_code(&e))?;
    // AdjustTokenPrivileges reports success even when the token lacks the privilege.
    if unsafe { GetLastError() } == ERROR_NOT_ALL_ASSIGNED {
        return Err(ERROR_NOT_ALL_ASSIGNED.0);
    }
    Ok(())
}

fn win32_code(e: &windows::core::Error) -> u32 {
    (e.code().0 & 0xFFFF) as u32
}
