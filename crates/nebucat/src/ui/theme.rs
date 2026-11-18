use gpui_kit::component::Theme as KitTheme;
use gpui_kit::{App, Global, Hsla, Rgba, WindowAppearance, px, rgb};
use nebucat_core::theme::{self as core_theme, Theme};

use crate::paths;

pub struct ActiveTheme(pub Theme);
impl Global for ActiveTheme {}

pub fn hsla(c: u32) -> Hsla {
    rgb(c).into()
}

pub fn hsla_alpha(c: u32, a: f32) -> Hsla {
    let mut h: Hsla = Rgba::from(rgb(c)).into();
    h.a = a;
    h
}

pub fn active(cx: &App) -> &Theme {
    &cx.global::<ActiveTheme>().0
}

fn is_dark(appearance: WindowAppearance) -> bool {
    matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark)
}

/// Resolves the configured theme name; `Err` carries the banner text and a fallback theme.
pub fn resolve(name: &str, appearance: WindowAppearance) -> Result<Theme, (String, Theme)> {
    let by_appearance = if is_dark(appearance) {
        core_theme::nebula_dark()
    } else {
        core_theme::nebula_light()
    };
    if name == "system" {
        return Ok(by_appearance);
    }
    let builtins = [core_theme::nebula_dark(), core_theme::nebula_light()];
    if let Some(t) = builtins.iter().find(|t| t.name == name) {
        return Ok(t.clone());
    }
    let path = paths::config_dir().join("themes").join(format!("{name}.toml"));
    let text = std::fs::read_to_string(&path)
        .map_err(|e| (format!("Theme \"{name}\": {e}"), by_appearance.clone()))?;
    core_theme::parse(&text, &builtins).map_err(|e| (format!("Theme \"{name}\": {e}"), by_appearance))
}

pub fn apply(t: &Theme, cx: &mut App) {
    cx.set_global(ActiveTheme(t.clone()));
    let t = t.clone();
    KitTheme::update(cx, move |k| {
        let c = &mut k.colors;
        c.background = hsla(t.background);
        c.popover = hsla(t.background);
        c.foreground = hsla(t.text);
        c.muted_foreground = hsla(t.muted);
        c.border = hsla(t.border);
        c.input = hsla(t.border);
        c.caret = hsla(t.accent_app);
        c.ring = hsla(t.accent_app);
        c.primary = hsla(t.accent_app);
        c.selection = hsla_alpha(t.accent_app, 0.30);
        c.list_active = hsla(t.surface);
        c.list_hover = hsla_alpha(t.surface, 0.5);
        c.accent = hsla(t.surface);
        c.accent_foreground = hsla(t.text);
        k.font_family = "Atkinson Hyperlegible Next".into();
        k.mono_font_family = "Atkinson Hyperlegible Mono".into();
        k.font_size = px(15.);
        k.radius = px(6.);
        k.radius_lg = px(10.);
    });
}
