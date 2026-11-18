use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::*;
use nebucat_core::ansi::{self, Color as AnsiColor};
use nebucat_core::config::Config;
use nebucat_core::history::{self, History};
use nebucat_core::json::{self, Tok};
use nebucat_core::plugin_proto::{Action as PluginAction, Item};
use nebucat_core::query::{self, Mode};
use nebucat_core::{calc, fuzzy, hotkey, url};
use windows::Win32::Foundation::{HANDLE, HWND};

use crate::apps::{self, AppEntry};
use crate::command::{self, CommandRun, Snapshot, Status};
use crate::files::{FileHit, Files, FilesError};
use crate::plugins::Plugins;
use crate::shell::{self, ShellEvent, ShellHandle};
use crate::ui::theme::{self as ui_theme, hsla, hsla_alpha};
use crate::{autostart, paths, settings, shellexec, window_ctl};

const WIDTH: f32 = 680.;
const BAR_H: f32 = 56.;
const ROW_H: f32 = 48.;
const LIST_PAD: f32 = 8.;
const BANNER_H: f32 = 36.;
const OUT_HEADER_H: f32 = 36.;
const OUT_FOOTER_H: f32 = 32.;
const OUT_LINE_H: f32 = 20.;
const OUT_PAD: f32 = 16.;
const INDEX_MAX_AGE: Duration = Duration::from_secs(300);
const MONO: &str = "Atkinson Hyperlegible Mono";

const PLACEHOLDER: &str = "Type an app, > for a command, / for files";
const EMPTY_HINT: &str =
    "Type an app name. Start with > to run a command or / to search files.";

#[derive(Clone, PartialEq)]
pub enum Source {
    App,
    Command,
    File,
    Plugin(SharedString),
    Calc,
    Web,
    Notice,
}

#[derive(Clone)]
pub enum RowAction {
    Launch { id: String, name: String },
    RunCommand(String),
    OpenPath(PathBuf),
    OpenUrl(String),
    Copy(String),
    PluginAction { keyword: String, data: serde_json::Value },
    Reindex,
    None,
}

#[derive(Clone)]
pub enum RowIcon {
    None,
    Image(Arc<RenderImage>),
    Path(PathBuf),
}

#[derive(Clone)]
pub struct Row {
    pub title: SharedString,
    pub subtitle: Option<SharedString>,
    pub highlight: Vec<u32>,
    pub source: Source,
    pub icon: RowIcon,
    pub action: RowAction,
}

impl Row {
    fn notice(text: impl Into<SharedString>, action: RowAction) -> Row {
        Row {
            title: text.into(),
            subtitle: None,
            highlight: Vec::new(),
            source: Source::Notice,
            icon: RowIcon::None,
            action,
        }
    }

    fn selectable(&self) -> bool {
        self.source != Source::Notice || !matches!(self.action, RowAction::None)
    }

    fn label(&self) -> Option<SharedString> {
        Some(match &self.source {
            Source::App => "App".into(),
            Source::Command => "Command".into(),
            Source::File => "File".into(),
            Source::Plugin(name) => name.clone(),
            Source::Calc => "Calc".into(),
            Source::Web => "Web".into(),
            Source::Notice => return None,
        })
    }
}

enum IndexState {
    Indexing,
    Ready,
    Failed(String),
}

#[derive(Clone, Copy)]
enum Seg {
    Text,
    Muted,
    Key,
    Ansi(u8),
    Rgb(u32),
}

type Line = Vec<(String, Seg, bool)>;

struct Output {
    cmd: String,
    run: CommandRun,
    snap: Snapshot,
    lines: Arc<Vec<Line>>,
    plain: String,
}

pub fn start(cx: &mut App, show_event: HANDLE) {
    settings::ensure_layout();
    let (config, config_banner) = match settings::load() {
        Ok(c) => (c, None),
        Err(e) => (Config::default(), Some(format!("config.toml: {e}. Using defaults."))),
    };
    let (hk, hotkey_banner) = match hotkey::parse(&config.hotkey) {
        Ok(h) => (h, None),
        Err(e) => (hotkey::parse("Alt+Space").expect("default hotkey"), Some(e)),
    };
    autostart::apply(config.autostart);

    let (tx, rx) = async_channel::unbounded();
    let shell = shell::start(hk, config.hotkey.clone(), show_event, tx);

    let options = WindowOptions {
        kind: WindowKind::PopUp,
        titlebar: None,
        show: false,
        focus: false,
        is_resizable: false,
        is_minimizable: false,
        is_movable: false,
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(0.), px(0.)),
            size(px(WIDTH), px(BAR_H)),
        ))),
        ..Default::default()
    };
    let (handle, launcher) = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| Launcher::new(config, shell, config_banner, hotkey_banner, window, cx))
    })
    .expect("open launcher window");

    cx.spawn(async move |cx: &mut AsyncApp| {
        while let Ok(ev) = rx.recv().await {
            let quit = matches!(ev, ShellEvent::Quit);
            let _ = cx.update_window(handle, |_, window, cx| {
                launcher.update(cx, |l, cx| l.on_shell(ev, window, cx));
            });
            if quit {
                break;
            }
        }
    })
    .detach();
}

pub struct Launcher {
    input: Entity<InputState>,
    hwnd: Option<HWND>,
    visible: bool,
    config: Config,
    shell: ShellHandle,
    rows: Vec<Row>,
    selected: usize,
    config_banner: Option<String>,
    hotkey_banner: Option<String>,
    theme_banner: Option<String>,
    plugin_banners: Vec<String>,
    apps: Vec<AppEntry>,
    index_state: IndexState,
    indexed_at: Option<Instant>,
    index_task: Option<Task<()>>,
    history: History,
    output: Option<Output>,
    output_task: Option<Task<()>>,
    out_scroll: UniformListScrollHandle,
    files: Files,
    plugins: Plugins,
    query_seq: u64,
    query_task: Option<Task<()>>,
    max_output_h: f32,
    last_h: f32,
    shown_at: Option<Instant>,
    animations: bool,
    _subs: Vec<Subscription>,
    _watcher: Option<notify::RecommendedWatcher>,
}

impl Launcher {
    fn new(
        config: Config,
        shell: ShellHandle,
        config_banner: Option<String>,
        hotkey_banner: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
        let mut subs = vec![
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change => {
                    this.output_task = None;
                    this.output = None;
                    this.refresh(window, cx);
                }
                InputEvent::PressEnter { secondary, shift } => {
                    if this.output.is_some() {
                        if *secondary {
                            this.open_output_in_terminal(window, cx);
                        }
                    } else {
                        this.activate(this.selected, *secondary, *shift, window, cx);
                    }
                }
                _ => {}
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                if this.visible && !window.is_window_active() {
                    this.hide(window, cx);
                }
            }),
            cx.observe_window_appearance(window, |this, window, cx| {
                this.apply_theme(window, cx);
            }),
        ];
        subs.shrink_to_fit();

        let hwnd = window_ctl::hwnd_of(window);
        let mut this = Self {
            input,
            hwnd,
            visible: false,
            config,
            shell,
            rows: Vec::new(),
            selected: 0,
            config_banner,
            hotkey_banner,
            theme_banner: None,
            plugin_banners: Vec::new(),
            apps: Vec::new(),
            index_state: IndexState::Indexing,
            indexed_at: None,
            index_task: None,
            history: apps::load_history(),
            output: None,
            output_task: None,
            out_scroll: UniformListScrollHandle::new(),
            files: Files::start(),
            plugins: Plugins::new(),
            query_seq: 0,
            query_task: None,
            max_output_h: 400.,
            last_h: BAR_H,
            shown_at: None,
            animations: window_ctl::animations_enabled(),
            _subs: subs,
            _watcher: None,
        };
        this.apply_theme(window, cx);
        this.plugin_banners = this.plugins.discover(&paths::config_dir().join("plugins"));
        this.reindex(window, cx);
        this.watch_config(window, cx);
        this.refresh(window, cx);
        this
    }

    fn apply_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let theme = match ui_theme::resolve(&self.config.theme, window.appearance()) {
            Ok(t) => {
                self.theme_banner = None;
                t
            }
            Err((msg, fallback)) => {
                self.theme_banner = Some(msg);
                fallback
            }
        };
        if let Some(h) = self.hwnd {
            window_ctl::apply_frame(h, theme.border);
        }
        cx.set_global(ui_theme::ActiveTheme(theme.clone()));
        // Kit colors are deferred so they land after gpui-kit's own appearance observer.
        cx.defer(move |cx| ui_theme::apply(&theme, cx));
        cx.notify();
    }

    fn on_shell(&mut self, ev: ShellEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            ShellEvent::Toggle => {
                if self.visible {
                    self.hide(window, cx)
                } else {
                    self.show(window, cx)
                }
            }
            ShellEvent::Show => self.show(window, cx),
            ShellEvent::ReloadIndex => self.reindex(window, cx),
            ShellEvent::OpenConfigFolder => {
                let dir = paths::config_dir().to_string_lossy().to_string();
                self.shell_call(move || shellexec::shell_execute("open", &dir, None), |_, _, _, _| {}, window, cx);
            }
            ShellEvent::Quit => cx.quit(),
            ShellEvent::HotkeyOk => {
                self.hotkey_banner = None;
                cx.notify();
            }
            ShellEvent::HotkeyError(msg) => {
                self.hotkey_banner = Some(msg);
                cx.notify();
            }
        }
    }

    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.shown_at = Some(Instant::now());
        self.animations = window_ctl::animations_enabled();
        if self.indexed_at.is_some_and(|t| t.elapsed() > INDEX_MAX_AGE) {
            self.reindex(window, cx);
        }
        self.refresh(window, cx);
        self.resize_to_content(window);
        let work = window_ctl::cursor_work_area();
        let work_h = (work.bottom - work.top) as f32;
        self.max_output_h = 0.6 * work_h / window.scale_factor();
        self.visible = true;
        let Some(hwnd) = self.hwnd else { return };
        let hwnd_raw = hwnd.0 as isize;
        let started = self.shown_at;
        // gpui's first activate applies its stored initial placement (origin 0,0) on a queued
        // task; queuing it before ours lets our centering win.
        window.activate_window();
        cx.spawn_in(window, async move |this, cx| {
            window_ctl::show(HWND(hwnd_raw as _), work);
            let _ = this.update_in(cx, |this, window, cx| {
                // The first activate restores gpui's creation size, so the cached height is stale.
                this.last_h = -1.;
                this.resize_to_content(window);
                this.input.update(cx, |s, cx| s.focus(window, cx));
                window.on_next_frame(move |_, _| {
                    if let Some(t) = started {
                        log::info!("show latency: {} ms", t.elapsed().as_millis());
                    }
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = false;
        if let Some(h) = self.hwnd {
            let raw = h.0 as isize;
            cx.spawn(async move |_, _| window_ctl::hide(HWND(raw as _))).detach();
        }
        self.output_task = None;
        self.output = None;
        self.input.update(cx, |s, cx| s.set_value("", window, cx));
        self.refresh(window, cx);
    }

    fn reindex(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.apps.is_empty() {
            self.index_state = IndexState::Indexing;
        }
        let rx = apps::spawn_index();
        self.index_task = Some(cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = rx.recv().await else { return };
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(list) => {
                        this.apps = list;
                        this.index_state = IndexState::Ready;
                    }
                    Err(e) => {
                        log::error!("app index: {e}");
                        if this.apps.is_empty() {
                            this.index_state = IndexState::Failed(e);
                        }
                    }
                }
                this.indexed_at = Some(Instant::now());
                this.refresh(window, cx);
            });
        }));
    }

    fn query_text(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.query_text(cx);
        self.query_seq += 1;
        self.query_task = None;
        let keywords = self.plugins.keywords();
        let rows = match query::parse(&text, &keywords) {
            Mode::Apps(q) => self.app_rows(&q),
            Mode::Command(c) if c.is_empty() => {
                vec![Row::notice("Type a command after >.", RowAction::None)]
            }
            Mode::Command(c) => vec![Row {
                title: c.clone().into(),
                subtitle: None,
                highlight: Vec::new(),
                source: Source::Command,
                icon: RowIcon::None,
                action: RowAction::RunCommand(c),
            }],
            Mode::Files(q) if q.is_empty() => vec![Row::notice(
                "Type a file name after /. Everything search syntax works, like ext:pdf.",
                RowAction::None,
            )],
            Mode::Files(q) => {
                self.search_files(q, window, cx);
                return;
            }
            Mode::Plugin { keyword, rest } => {
                self.query_plugin(keyword, rest, window, cx);
                return;
            }
        };
        self.set_rows(rows, window, cx);
    }

    fn set_rows(&mut self, rows: Vec<Row>, window: &mut Window, cx: &mut Context<Self>) {
        self.rows = rows;
        self.selected = self.rows.iter().position(Row::selectable).unwrap_or(0);
        self.resize_to_content(window);
        cx.notify();
    }

    fn app_row(&self, app: &AppEntry, highlight: Vec<u32>) -> Row {
        Row {
            title: app.name.clone().into(),
            subtitle: None,
            highlight,
            source: Source::App,
            icon: app.icon.clone().map_or(RowIcon::None, RowIcon::Image),
            action: RowAction::Launch { id: app.id.clone(), name: app.name.clone() },
        }
    }

    fn app_rows(&self, q: &str) -> Vec<Row> {
        let max = self.config.max_results.clamp(1, 9);
        if q.is_empty() {
            return match &self.index_state {
                IndexState::Indexing => vec![Row::notice("Indexing apps…", RowAction::None)],
                IndexState::Failed(e) => vec![index_failed(e)],
                IndexState::Ready => {
                    let rows: Vec<Row> = self
                        .history
                        .recent(max)
                        .into_iter()
                        .filter_map(|id| self.apps.iter().find(|a| a.id == id))
                        .map(|a| self.app_row(a, Vec::new()))
                        .collect();
                    if rows.is_empty() {
                        vec![Row::notice(EMPTY_HINT, RowAction::None)]
                    } else {
                        rows
                    }
                }
            };
        }

        let mut rows = Vec::new();
        if let Some(v) = calc::eval(q) {
            let value = calc::format(v);
            rows.push(Row {
                title: format!("= {value}").into(),
                subtitle: Some("Enter copies the result".into()),
                highlight: Vec::new(),
                source: Source::Calc,
                icon: RowIcon::None,
                action: RowAction::Copy(value),
            });
        }
        match &self.index_state {
            IndexState::Indexing => rows.push(Row::notice("Indexing apps…", RowAction::None)),
            IndexState::Failed(e) => rows.push(index_failed(e)),
            IndexState::Ready => {
                let names: Vec<&str> = self.apps.iter().map(|a| a.name.as_str()).collect();
                let boosts: Vec<f64> = self
                    .apps
                    .iter()
                    .map(|a| self.history.apps.get(&a.id).map_or(0.0, |e| history::boost(e.count)))
                    .collect();
                let limit = max.saturating_sub(rows.len()).max(1);
                for hit in fuzzy::rank(q, &names, &boosts, limit) {
                    rows.push(self.app_row(&self.apps[hit.index], hit.indices));
                }
            }
        }
        if rows.is_empty() {
            rows.push(Row::notice(
                format!("No apps match \"{q}\". Start with > to run it as a command."),
                RowAction::None,
            ));
            if self.config.web.enabled {
                rows.push(Row {
                    title: format!("Search the web for \"{q}\"").into(),
                    subtitle: None,
                    highlight: Vec::new(),
                    source: Source::Web,
                    icon: RowIcon::None,
                    action: RowAction::OpenUrl(
                        self.config.web.url.replace("{query}", &url::encode_query(q)),
                    ),
                });
            }
        }
        rows
    }

    fn search_files(&mut self, q: String, window: &mut Window, cx: &mut Context<Self>) {
        let seq = self.query_seq;
        let max = self.config.files.max_results.max(1) as u32;
        let files_rx_q = q.clone();
        let rx = {
            let files = &self.files;
            move || files.search(files_rx_q, max)
        };
        // Debounced: the request is issued only if no keystroke follows within 120 ms.
        let rx_factory = rx();
        let task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(120)).await;
            let alive = this
                .update(cx, |this, _| this.query_seq == seq)
                .unwrap_or(false);
            if !alive {
                return;
            }
            let slow = cx.background_executor().timer(Duration::from_millis(300));
            let result = futures_select(rx_factory, slow, &this, cx, seq).await;
            let Some(result) = result else { return };
            let _ = this.update_in(cx, |this, window, cx| {
                if this.query_seq != seq {
                    return;
                }
                let rows = match result {
                    Ok(hits) if hits.is_empty() => {
                        vec![Row::notice(format!("No files match \"{q}\"."), RowAction::None)]
                    }
                    Ok(hits) => hits.into_iter().map(|h| file_row(&q, h)).collect(),
                    Err(FilesError::NotRunning) => vec![Row::notice(
                        "Everything isn't running. Install it with winget install voidtools.Everything, then start it.",
                        RowAction::None,
                    )],
                    Err(FilesError::DllMissing) => vec![Row::notice(
                        "Everything64.dll is missing next to nebucat.exe. Run scripts/win.sh build.",
                        RowAction::None,
                    )],
                    Err(FilesError::Other(e)) => vec![Row::notice(e, RowAction::None)],
                };
                this.set_rows(rows, window, cx);
            });
        });
        self.query_task = Some(task);
    }

    fn query_plugin(&mut self, keyword: String, rest: String, window: &mut Window, cx: &mut Context<Self>) {
        let seq = self.query_seq;
        let Some(info) = self.plugins.info(&keyword).cloned() else { return };
        let task = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(80)).await;
            let rx = this.update(cx, |this, _| {
                (this.query_seq == seq).then(|| this.plugins.query(&keyword, &rest))
            });
            let Ok(Some(rx)) = rx else { return };
            let Ok(result) = rx.recv().await else { return };
            let _ = this.update_in(cx, |this, window, cx| {
                if this.query_seq != seq {
                    return;
                }
                let rows = match result {
                    Ok(items) if items.is_empty() => {
                        vec![Row::notice(format!("{}: no results.", info.name), RowAction::None)]
                    }
                    Ok(items) => items
                        .into_iter()
                        .take(9)
                        .map(|item| this.plugin_row(&keyword, &info.name, item))
                        .collect(),
                    Err(msg) => vec![Row::notice(msg, RowAction::None)],
                };
                this.set_rows(rows, window, cx);
            });
        });
        self.query_task = Some(task);
    }

    fn plugin_row(&self, keyword: &str, name: &str, item: Item) -> Row {
        let action = match item.action {
            Some(PluginAction::OpenUrl { url }) => RowAction::OpenUrl(url),
            Some(PluginAction::OpenPath { path }) => RowAction::OpenPath(path.into()),
            Some(PluginAction::Copy { text }) => RowAction::Copy(text),
            Some(PluginAction::Run { command }) => RowAction::RunCommand(command),
            Some(PluginAction::Plugin { data }) => {
                RowAction::PluginAction { keyword: keyword.to_string(), data }
            }
            None => RowAction::None,
        };
        let icon = item
            .icon
            .as_deref()
            .map_or(RowIcon::None, |i| RowIcon::Path(self.plugins.icon_path(keyword, i)));
        Row {
            title: item.title.into(),
            subtitle: item.subtitle.map(Into::into),
            highlight: item.highlight.unwrap_or_default(),
            source: Source::Plugin(name.to_string().into()),
            icon,
            action,
        }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.rows.len();
        if n == 0 || !self.rows.iter().any(Row::selectable) {
            return;
        }
        let mut i = self.selected as isize;
        loop {
            i = (i + delta).rem_euclid(n as isize);
            if self.rows[i as usize].selectable() {
                break;
            }
        }
        self.selected = i as usize;
        cx.notify();
    }

    fn nth_actionable(&self, n: usize) -> Option<usize> {
        self.rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.selectable())
            .nth(n)
            .map(|(i, _)| i)
    }

    /// ShellExecuteW pumps messages, and the window procedure re-enters gpui, so the call
    /// runs from a queued task instead of inside the current update.
    fn shell_call(
        &mut self,
        call: impl FnOnce() -> Result<(), i32> + 'static,
        done: impl FnOnce(&mut Self, Result<(), i32>, &mut Window, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            let r = call();
            let _ = this.update_in(cx, |this, window, cx| done(this, r, window, cx));
        })
        .detach();
    }

    fn activate(&mut self, index: usize, secondary: bool, shift: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(index).cloned() else { return };
        if !row.selectable() {
            return;
        }
        match row.action {
            RowAction::Launch { id, name } => {
                let admin = secondary && shift;
                let launch_id = id.clone();
                self.shell_call(
                    move || apps::launch(&launch_id, admin),
                    move |this, r, window, cx| match r {
                        Ok(()) => {
                            let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
                            this.history.record(&id, now);
                            apps::save_history(&this.history);
                            this.hide(window, cx);
                        }
                        Err(code) => this.set_rows(
                            vec![Row::notice(format!("Couldn't start {name} (error {code})."), RowAction::None)],
                            window,
                            cx,
                        ),
                    },
                    window,
                    cx,
                );
            }
            RowAction::RunCommand(cmd) => {
                if secondary {
                    self.shell_call(
                        move || command::open_in_terminal(&cmd),
                        |this, _, window, cx| this.hide(window, cx),
                        window,
                        cx,
                    );
                } else {
                    self.run_command(cmd, window, cx);
                }
            }
            RowAction::OpenPath(path) => {
                let p = path.to_string_lossy().to_string();
                let target = p.clone();
                self.shell_call(
                    move || {
                        if secondary {
                            shellexec::shell_execute("open", "explorer.exe", Some(&format!("/select,\"{target}\"")))
                        } else {
                            shellexec::shell_execute("open", &target, None)
                        }
                    },
                    move |this, r, window, cx| match r {
                        Ok(()) => this.hide(window, cx),
                        Err(code) => this.set_rows(
                            vec![Row::notice(format!("Couldn't open {p} (error {code})."), RowAction::None)],
                            window,
                            cx,
                        ),
                    },
                    window,
                    cx,
                );
            }
            RowAction::OpenUrl(u) => {
                cx.open_url(&u);
                self.hide(window, cx);
            }
            RowAction::Copy(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                self.hide(window, cx);
            }
            RowAction::PluginAction { keyword, data } => {
                let rx = self.plugins.action(&keyword, &data);
                self.query_task = Some(cx.spawn_in(window, async move |this, cx| {
                    let Ok(result) = rx.recv().await else { return };
                    let _ = this.update_in(cx, |this, window, cx| match result {
                        Ok(r) => {
                            if let Some(q) = r.query {
                                this.input.update(cx, |s, cx| s.set_value(q, window, cx));
                                this.refresh(window, cx);
                            } else if r.hide {
                                this.hide(window, cx);
                            }
                        }
                        Err(msg) => this.set_rows(vec![Row::notice(msg, RowAction::None)], window, cx),
                    });
                }));
            }
            RowAction::Reindex => {
                self.index_state = IndexState::Indexing;
                self.reindex(window, cx);
                self.refresh(window, cx);
            }
            RowAction::None => {}
        }
    }

    fn run_command(&mut self, cmd: String, window: &mut Window, cx: &mut Context<Self>) {
        let run = CommandRun::start(&cmd, self.config.command.shell, self.config.command.timeout_secs);
        let snap = run.snapshot();
        self.output = Some(Output { cmd, run, snap, lines: Arc::new(Vec::new()), plain: String::new() });
        self.update_output();
        self.output_task = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(50)).await;
                let running = this.update_in(cx, |this, window, cx| {
                    let running = this.update_output();
                    this.resize_to_content(window);
                    cx.notify();
                    running
                });
                if !matches!(running, Ok(true)) {
                    break;
                }
            }
        }));
        self.resize_to_content(window);
        cx.notify();
    }

    /// Returns true while the command is still running.
    fn update_output(&mut self) -> bool {
        let Some(out) = self.output.as_mut() else { return false };
        let prev_len = out.snap.output.len();
        let prev_running = out.snap.status == Status::Running;
        out.snap = out.run.snapshot();
        let running = out.snap.status == Status::Running;
        if out.snap.output.len() == prev_len && running == prev_running && !out.lines.is_empty() {
            return running;
        }
        let mut lines: Vec<Line> = Vec::new();
        let pretty = (!running)
            .then(|| json::pretty(&String::from_utf8_lossy(&out.snap.stdout)))
            .flatten();
        if let Some(tokens) = pretty {
            for tl in tokens {
                lines.push(
                    tl.into_iter()
                        .map(|(tok, text)| {
                            let seg = match tok {
                                Tok::Key => Seg::Key,
                                Tok::Punct => Seg::Muted,
                                _ => Seg::Text,
                            };
                            (text, seg, false)
                        })
                        .collect(),
                );
            }
        } else {
            for l in ansi::parse_lines(&out.snap.output) {
                lines.push(
                    l.into_iter()
                        .map(|s| {
                            let seg = match s.fg {
                                AnsiColor::Default => Seg::Text,
                                AnsiColor::Ansi(n) => Seg::Ansi(n),
                                AnsiColor::Rgb(r, g, b) => {
                                    Seg::Rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
                                }
                            };
                            (s.text, seg, s.bold)
                        })
                        .collect(),
                );
            }
        }
        if out.snap.truncated {
            if let Some(last) = lines.iter_mut().rev().find(|l| !l.is_empty()) {
                for seg in last.iter_mut() {
                    seg.1 = Seg::Muted;
                }
            }
        }
        if let Status::TimedOut { .. } = out.snap.status {
            lines.push(vec![(
                "Ctrl+Enter opens it in Windows Terminal for interactive commands.".into(),
                Seg::Muted,
                false,
            )]);
        }
        out.plain = lines
            .iter()
            .map(|l| l.iter().map(|(t, _, _)| t.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        let n = lines.len();
        out.lines = Arc::new(lines);
        if running && n > 0 {
            self.out_scroll.scroll_to_item(n - 1, ScrollStrategy::Bottom);
        }
        running
    }

    fn open_output_in_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(out) = &self.output {
            let cmd = out.cmd.clone();
            self.shell_call(
                move || command::open_in_terminal(&cmd),
                |this, _, window, cx| this.hide(window, cx),
                window,
                cx,
            );
        }
    }

    fn copy_selection(&mut self, cx: &mut Context<Self>) {
        let text = if let Some(out) = &self.output {
            Some(out.plain.clone())
        } else {
            self.rows.get(self.selected).and_then(|r| match &r.action {
                RowAction::Launch { name, .. } => Some(name.clone()),
                RowAction::RunCommand(c) => Some(c.clone()),
                RowAction::OpenPath(p) => Some(p.to_string_lossy().to_string()),
                RowAction::OpenUrl(u) => Some(u.clone()),
                RowAction::Copy(t) => Some(t.clone()),
                _ => None,
            })
        };
        if let Some(t) = text {
            cx.write_to_clipboard(ClipboardItem::new_string(t));
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = &ev.keystroke.modifiers;
        let key = ev.keystroke.key.as_str();
        let handled = match (key, m.control, m.shift, m.alt) {
            ("up", false, _, false) => {
                self.move_selection(-1, cx);
                true
            }
            ("down", false, _, false) => {
                self.move_selection(1, cx);
                true
            }
            ("tab", false, shift, false) => {
                self.move_selection(if shift { -1 } else { 1 }, cx);
                true
            }
            ("escape", false, false, false) => {
                match &self.output {
                    Some(out) if out.snap.status == Status::Running => {
                        out.run.cancel();
                        self.update_output();
                        cx.notify();
                    }
                    Some(_) => {
                        self.output_task = None;
                        self.output = None;
                        self.refresh(window, cx);
                    }
                    None => self.hide(window, cx),
                }
                true
            }
            ("c", true, true, false) => {
                self.copy_selection(cx);
                true
            }
            ("r", true, false, false) => {
                self.index_state = IndexState::Indexing;
                self.reindex(window, cx);
                self.refresh(window, cx);
                true
            }
            (d, true, false, false) if d.len() == 1 && ("1"..="9").contains(&d) => {
                let n = d.parse::<usize>().unwrap_or(1) - 1;
                if self.output.is_none() {
                    if let Some(i) = self.nth_actionable(n) {
                        self.selected = i;
                        self.activate(i, false, false, window, cx);
                    }
                }
                true
            }
            _ => false,
        };
        if handled {
            cx.stop_propagation();
        }
    }

    fn watch_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use notify::{RecursiveMode, Watcher};
        let (tx, rx) = async_channel::unbounded::<Vec<PathBuf>>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                let _ = tx.send_blocking(ev.paths);
            }
        });
        let mut watcher = match watcher {
            Ok(w) => w,
            Err(e) => {
                log::error!("config watcher: {e}");
                return;
            }
        };
        if let Err(e) = watcher.watch(&paths::config_dir(), RecursiveMode::Recursive) {
            log::error!("watch config dir: {e}");
            return;
        }
        self._watcher = Some(watcher);
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(first) = rx.recv().await {
                let mut changed = first;
                // Editors save in several writes; coalesce them.
                cx.background_executor().timer(Duration::from_millis(200)).await;
                while let Ok(more) = rx.try_recv() {
                    changed.extend(more);
                }
                let ok = this.update_in(cx, |this, window, cx| this.on_config_change(changed, window, cx));
                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn on_config_change(&mut self, changed: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let root = paths::config_dir();
        let rel = |p: &PathBuf| p.strip_prefix(&root).map(|r| r.to_path_buf()).ok();
        let mut config_changed = false;
        let mut theme_changed = false;
        let mut plugins_changed = false;
        for p in &changed {
            let Some(r) = rel(p) else { continue };
            let first = r.components().next().map(|c| c.as_os_str().to_string_lossy().to_string());
            match first.as_deref() {
                Some("config.toml") => config_changed = true,
                Some("themes") => {
                    if r.file_stem().is_some_and(|s| s.to_string_lossy() == self.config.theme) {
                        theme_changed = true;
                    }
                }
                Some("plugins") => plugins_changed = true,
                _ => {}
            }
        }
        if config_changed {
            match settings::load() {
                Err(e) => self.config_banner = Some(format!("config.toml: {e}. Using previous settings.")),
                Ok(new) => {
                    self.config_banner = None;
                    if new.hotkey != self.config.hotkey {
                        match hotkey::parse(&new.hotkey) {
                            Ok(hk) => self.shell.set_hotkey(hk, new.hotkey.clone()),
                            Err(e) => self.hotkey_banner = Some(e),
                        }
                    }
                    if new.autostart != self.config.autostart {
                        autostart::apply(new.autostart);
                    }
                    theme_changed |= new.theme != self.config.theme;
                    self.config = new;
                }
            }
        }
        if theme_changed {
            self.apply_theme(window, cx);
        }
        if plugins_changed {
            self.plugin_banners = self.plugins.discover(&root.join("plugins"));
        }
        if config_changed || plugins_changed {
            self.refresh(window, cx);
        }
        self.resize_to_content(window);
        cx.notify();
    }

    fn banners(&self) -> Vec<(&'static str, String)> {
        let mut v = Vec::new();
        if let Some(b) = &self.config_banner {
            v.push(("Config", b.clone()));
        }
        if let Some(b) = &self.hotkey_banner {
            v.push(("Hotkey", b.clone()));
        }
        if let Some(b) = &self.theme_banner {
            v.push(("Theme", b.clone()));
        }
        for b in &self.plugin_banners {
            v.push(("Plugin", b.clone()));
        }
        v
    }

    fn output_body_h(&self) -> f32 {
        let Some(out) = &self.output else { return 0. };
        let content = out.lines.len().max(1) as f32 * OUT_LINE_H + 2. * OUT_PAD;
        let cap = (self.max_output_h - OUT_HEADER_H - OUT_FOOTER_H).max(OUT_LINE_H * 3.);
        content.min(cap)
    }

    fn content_height(&self) -> f32 {
        let mut h = BAR_H + self.banners().len() as f32 * BANNER_H;
        if self.output.is_some() {
            h += 1. + OUT_HEADER_H + self.output_body_h() + OUT_FOOTER_H;
        } else if !self.rows.is_empty() {
            h += 1. + 2. * LIST_PAD + self.rows.len() as f32 * ROW_H;
        }
        h
    }

    fn resize_to_content(&mut self, window: &mut Window) {
        let h = self.content_height();
        if (h - self.last_h).abs() > 0.5 {
            self.last_h = h;
            window.resize(size(px(WIDTH), px(h)));
        }
    }

    fn render_row(&self, i: usize, row: &Row, hint: Option<usize>, cx: &mut Context<Self>) -> AnyElement {
        let t = ui_theme::active(cx).clone();
        let accent = match row.source {
            Source::Command => t.accent_command,
            Source::File | Source::Plugin(_) => t.accent_extension,
            _ => t.accent_app,
        };
        let selectable = row.selectable();
        let selected = selectable && i == self.selected;

        let title_color = if row.source == Source::Notice { t.muted } else { t.text };
        let highlights = highlight_runs(&row.title, &row.highlight, hsla(accent));
        let title = div()
            .text_size(px(15.))
            .text_color(hsla(title_color))
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(StyledText::new(row.title.clone()).with_highlights(highlights));

        let mut text_col = div().flex().flex_col().flex_1().min_w_0().child(title);
        if let Some(sub) = &row.subtitle {
            text_col = text_col.child(
                div()
                    .text_size(px(12.))
                    .text_color(hsla(t.muted))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(sub.clone()),
            );
        }

        let mut el = div()
            .id(("row", i))
            .h(px(ROW_H))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(12.))
            .rounded(px(6.));

        if row.source != Source::Notice {
            // Fixed slot keeps titles aligned and stops layout shift when icons arrive.
            let slot = div().size(px(24.)).flex_none().flex().items_center().justify_center();
            let slot = match (&row.icon, &row.source) {
                (RowIcon::Image(img_data), _) => slot.child(img(img_data.clone()).size(px(24.))),
                (RowIcon::Path(p), _) => slot.child(img(p.clone()).size(px(24.))),
                (_, Source::Command) => slot.child(
                    div().font_family(MONO).text_size(px(15.)).text_color(hsla(t.accent_command)).child(">"),
                ),
                (_, Source::Calc) => slot.child(
                    div().font_family(MONO).text_size(px(15.)).text_color(hsla(t.accent_app)).child("="),
                ),
                _ => slot,
            };
            el = el.child(slot);
        }
        el = el.child(text_col);

        if let Some(label) = row.label() {
            el = el.child(
                div()
                    .flex_none()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(hsla(accent))
                    .child(label),
            );
        }
        if let Some(n) = hint {
            el = el.child(
                div()
                    .flex_none()
                    .w(px(44.))
                    .flex()
                    .justify_end()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .text_color(hsla(t.muted))
                    .child(format!("Ctrl {n}")),
            );
        }

        if selected {
            el = el.bg(hsla(t.surface));
        }
        if selectable {
            el = el
                .cursor_pointer()
                .hover(|s| s.bg(hsla_alpha(t.surface, 0.5)))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.selected = i;
                    this.activate(i, false, false, window, cx);
                }));
        }
        el.into_any_element()
    }

    fn render_output(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = ui_theme::active(cx).clone();
        let Some(out) = &self.output else { return div().into_any_element() };
        let status: AnyElement = match &out.snap.status {
            Status::Running => div()
                .text_color(hsla(t.muted))
                .child(format!(
                    "Running… {} s · Esc to cancel",
                    out.run_elapsed_secs()
                ))
                .into_any_element(),
            Status::Exited { code: 0, secs } => div()
                .text_color(hsla(t.muted))
                .child(format!("exit 0 · {secs:.1} s"))
                .into_any_element(),
            Status::Exited { code, secs } => div()
                .flex()
                .gap(px(4.))
                .child(div().text_color(hsla(t.accent_command)).child(format!("exit {code}")))
                .child(div().text_color(hsla(t.muted)).child(format!("· {secs:.1} s")))
                .into_any_element(),
            Status::TimedOut { secs } => div()
                .text_color(hsla(t.muted))
                .child(format!("Stopped after {secs} s."))
                .into_any_element(),
            Status::Cancelled => div().text_color(hsla(t.muted)).child("Cancelled.").into_any_element(),
            Status::Failed(e) => div()
                .text_color(hsla(t.accent_command))
                .child(format!("Couldn't start the shell: {e}"))
                .into_any_element(),
        };
        let header = div()
            .h(px(OUT_HEADER_H))
            .px(px(20.))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .font_family(MONO)
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(8.))
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(div().text_color(hsla(t.accent_command)).child(">"))
                    .child(div().text_color(hsla(t.text)).text_ellipsis().child(out.cmd.clone())),
            )
            .child(div().flex_none().child(status));

        let lines = out.lines.clone();
        let theme = t.clone();
        let body_h = self.output_body_h();
        let list = uniform_list("output", lines.len(), move |range, _, _| {
            range
                .map(|ix| render_line(&lines[ix], &theme))
                .collect::<Vec<_>>()
        })
        .track_scroll(&self.out_scroll)
        .h(px(body_h - 2. * OUT_PAD))
        .font_family(MONO)
        .text_size(px(13.));
        let body = div().h(px(body_h)).px(px(OUT_PAD)).py(px(OUT_PAD)).child(list);
        let body: AnyElement = if self.animations && !out.lines.is_empty() {
            body.with_animation(
                "output-fade",
                Animation::new(Duration::from_millis(120)),
                |el, delta| el.opacity(delta),
            )
            .into_any_element()
        } else {
            body.into_any_element()
        };
        let footer = div()
            .h(px(OUT_FOOTER_H))
            .px(px(20.))
            .flex()
            .items_center()
            .font_family(MONO)
            .text_size(px(11.))
            .text_color(hsla(t.muted))
            .child("Ctrl+Shift+C copy · Ctrl+Enter open in Terminal · Esc back");
        div()
            .flex()
            .flex_col()
            .child(div().h(px(1.)).bg(hsla(t.border)))
            .child(header)
            .child(body)
            .child(footer)
            .into_any_element()
    }
}

impl Output {
    fn run_elapsed_secs(&self) -> u64 {
        self.run.elapsed().as_secs()
    }
}

impl Render for Launcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.resize_to_content(window);
        let t = ui_theme::active(cx).clone();

        let bar = div()
            .h(px(BAR_H))
            .px(px(20.))
            .flex()
            .items_center()
            .child(
                Input::new(&self.input)
                    .appearance(false)
                    .bordered(false)
                    .focus_bordered(false)
                    .text_size(px(22.)),
            );

        let mut root = div()
            .key_context("Launcher")
            .size_full()
            .flex()
            .flex_col()
            .bg(hsla(t.background))
            .text_color(hsla(t.text))
            .font_family("Atkinson Hyperlegible Next")
            .capture_key_down(cx.listener(Self::on_key))
            .child(bar);

        for (label, msg) in self.banners() {
            root = root.child(
                div()
                    .h(px(BANNER_H))
                    .px(px(20.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .border_t_1()
                    .border_color(hsla(t.border))
                    .child(
                        div().flex_none().font_family(MONO).text_size(px(11.)).text_color(hsla(t.muted)).child(label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(msg),
                    ),
            );
        }

        if self.output.is_some() {
            root = root.child(self.render_output(cx));
        } else if !self.rows.is_empty() {
            let mut list = div().flex().flex_col().p(px(LIST_PAD));
            let mut actionable = 0;
            let rows = self.rows.clone();
            for (i, row) in rows.iter().enumerate() {
                let hint = row.selectable().then(|| {
                    actionable += 1;
                    actionable
                });
                list = list.child(self.render_row(i, row, hint.filter(|n| *n <= 9), cx));
            }
            root = root.child(div().h(px(1.)).bg(hsla(t.border))).child(list);
        }
        root
    }
}

fn index_failed(e: &str) -> Row {
    Row::notice(
        format!("Couldn't read the Apps folder: {e}. Press Ctrl+R to retry."),
        RowAction::Reindex,
    )
}

fn file_row(q: &str, hit: FileHit) -> Row {
    let highlight = fuzzy::rank(q, &[hit.name.as_str()], &[0.0], 1)
        .into_iter()
        .next()
        .map(|h| h.indices)
        .unwrap_or_default();
    Row {
        title: hit.name.into(),
        subtitle: Some(hit.parent.into()),
        highlight,
        source: Source::File,
        icon: hit.icon.map_or(RowIcon::None, RowIcon::Image),
        action: RowAction::OpenPath(hit.path),
    }
}

/// Converts char indices into byte ranges styled SemiBold in the source accent.
fn highlight_runs(text: &str, indices: &[u32], color: Hsla) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let style = HighlightStyle {
        color: Some(color),
        font_weight: Some(FontWeight::SEMIBOLD),
        ..Default::default()
    };
    let mut runs: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    let mut want = indices.iter().copied().peekable();
    for (ci, (bi, ch)) in text.char_indices().enumerate() {
        while want.peek().is_some_and(|w| (*w as usize) < ci) {
            want.next();
        }
        if want.peek().is_some_and(|w| *w as usize == ci) {
            let end = bi + ch.len_utf8();
            match runs.last_mut() {
                Some((r, _)) if r.end == bi => r.end = end,
                _ => runs.push((bi..end, style)),
            }
        }
    }
    runs
}

fn seg_color(seg: Seg, t: &nebucat_core::theme::Theme) -> Hsla {
    match seg {
        Seg::Text => hsla(t.text),
        Seg::Muted => hsla(t.muted),
        Seg::Key => hsla(t.accent_command),
        Seg::Ansi(n) => hsla(t.ansi[(n as usize).min(15)]),
        Seg::Rgb(c) => hsla(c),
    }
}

fn render_line(line: &Line, t: &nebucat_core::theme::Theme) -> AnyElement {
    let mut text = String::new();
    let mut runs = Vec::new();
    for (s, seg, bold) in line {
        let start = text.len();
        text.push_str(s);
        runs.push((
            start..text.len(),
            HighlightStyle {
                color: Some(seg_color(*seg, t)),
                font_weight: bold.then_some(FontWeight::SEMIBOLD),
                ..Default::default()
            },
        ));
    }
    div()
        .h(px(OUT_LINE_H))
        .whitespace_nowrap()
        .overflow_hidden()
        .child(StyledText::new(text).with_highlights(runs))
        .into_any_element()
}

/// Awaits the file search; shows "Searching…" if it takes longer than 300 ms.
async fn futures_select(
    rx: async_channel::Receiver<Result<Vec<FileHit>, FilesError>>,
    slow: Task<()>,
    this: &WeakEntity<Launcher>,
    cx: &mut AsyncWindowContext,
    seq: u64,
) -> Option<Result<Vec<FileHit>, FilesError>> {
    use futures_lite::future::FutureExt;
    let first = async { Some(rx.recv().await.ok()) }
        .or(async {
            slow.await;
            None
        })
        .await;
    match first {
        Some(r) => r,
        None => {
            let _ = this.update_in(cx, |this, window, cx| {
                if this.query_seq == seq {
                    this.set_rows(vec![Row::notice("Searching…", RowAction::None)], window, cx);
                }
            });
            rx.recv().await.ok()
        }
    }
}
