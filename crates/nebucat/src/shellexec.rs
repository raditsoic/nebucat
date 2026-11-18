use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR};

/// `Err` holds the ShellExecuteW return code when it signals failure (<= 32).
pub fn shell_execute(verb: &str, file: &str, params: Option<&str>) -> Result<(), i32> {
    let verb = HSTRING::from(verb);
    let file = HSTRING::from(file);
    let params = params.map(HSTRING::from);
    let r = unsafe {
        ShellExecuteW(
            None,
            &verb,
            &file,
            params.as_ref().map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
            None,
            SW_SHOWNORMAL,
        )
    };
    let code = r.0 as isize;
    if code <= 32 { Err(code as i32) } else { Ok(()) }
}
