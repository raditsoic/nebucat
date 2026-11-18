#[cfg(debug_assertions)]
pub fn apply(_enabled: bool) {
    log::info!("autostart skipped in debug build");
}

#[cfg(not(debug_assertions))]
pub fn apply(enabled: bool) {
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
        RegSetValueExW,
    };
    use windows::core::{HSTRING, w};

    let mut key = HKEY::default();
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            None,
            KEY_SET_VALUE,
            &mut key,
        )
    };
    if opened.is_err() {
        log::error!("autostart: open Run key: {opened:?}");
        return;
    }
    if enabled {
        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                log::error!("autostart: current_exe: {e}");
                return;
            }
        };
        let value = format!("\"{}\"", exe.display());
        let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };
        let r = unsafe { RegSetValueExW(key, w!("nebucat"), None, REG_SZ, Some(bytes)) };
        if r.is_err() {
            log::error!("autostart: set value: {r:?}");
        }
    } else {
        // Missing value is the desired state, so its error is ignored.
        let _ = unsafe { RegDeleteValueW(key, &HSTRING::from("nebucat")) };
    }
    let _ = unsafe { RegCloseKey(key) };
}
