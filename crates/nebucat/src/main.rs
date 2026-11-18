#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("nebucat runs on Windows only");
    std::process::exit(1);
}

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod apps;
#[cfg(windows)]
mod autostart;
#[cfg(windows)]
mod command;
#[cfg(windows)]
mod files;
#[cfg(windows)]
mod icons;
#[cfg(windows)]
mod plugins;
#[cfg(windows)]
mod shellexec;
#[cfg(windows)]
mod paths;
#[cfg(windows)]
mod settings;
#[cfg(windows)]
mod shell;
#[cfg(windows)]
mod ui;
#[cfg(windows)]
mod window_ctl;

#[cfg(windows)]
fn main() {
    use std::borrow::Cow;
    use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows::Win32::System::Threading::{
        CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, OpenEventW, SetEvent,
    };
    use windows::core::w;

    let mutex = unsafe { CreateMutexW(None, true, w!("Local\\nebucat.single-instance")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        if let Ok(ev) = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, w!("Local\\nebucat.show")) } {
            let _ = unsafe { SetEvent(ev) };
        }
        return;
    }
    let show_event = unsafe { CreateEventW(None, false, false, w!("Local\\nebucat.show")) }
        .expect("create show event");
    let _keep_mutex = mutex;

    paths::init_logging();
    log::info!("nebucat {} starting", env!("CARGO_PKG_VERSION"));

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            cx.text_system()
                .add_fonts(vec![
                    Cow::Borrowed(include_bytes!("../../../assets/fonts/AtkinsonHyperlegibleNext-Regular.ttf").as_slice()),
                    Cow::Borrowed(include_bytes!("../../../assets/fonts/AtkinsonHyperlegibleNext-Medium.ttf").as_slice()),
                    Cow::Borrowed(include_bytes!("../../../assets/fonts/AtkinsonHyperlegibleNext-SemiBold.ttf").as_slice()),
                    Cow::Borrowed(include_bytes!("../../../assets/fonts/AtkinsonHyperlegibleMono-Regular.ttf").as_slice()),
                    Cow::Borrowed(include_bytes!("../../../assets/fonts/AtkinsonHyperlegibleMono-SemiBold.ttf").as_slice()),
                ])
                .expect("register fonts");
            gpui_kit::init(cx);
            app::start(cx, show_event);
        });
}
