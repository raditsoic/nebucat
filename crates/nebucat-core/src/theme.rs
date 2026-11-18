use toml::{Table, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    pub background: u32,
    pub surface: u32,
    pub border: u32,
    pub text: u32,
    pub muted: u32,
    pub accent_app: u32,
    pub accent_command: u32,
    pub accent_extension: u32,
    pub ansi: [u32; 16],
}

const COLOR_KEYS: [&str; 8] = [
    "background",
    "surface",
    "border",
    "text",
    "muted",
    "accent_app",
    "accent_command",
    "accent_extension",
];

const ANSI_KEYS: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright_black",
    "bright_red",
    "bright_green",
    "bright_yellow",
    "bright_blue",
    "bright_magenta",
    "bright_cyan",
    "bright_white",
];

const DEFAULT_BASE: &str = "nebula-dark";

pub fn nebula_dark() -> Theme {
    parse_full(include_str!("../../../themes/nebula-dark.toml"))
        .expect("built-in nebula-dark theme is valid")
}

pub fn nebula_light() -> Theme {
    parse_full(include_str!("../../../themes/nebula-light.toml"))
        .expect("built-in nebula-light theme is valid")
}

pub fn parse(toml: &str, builtins: &[Theme]) -> Result<Theme, String> {
    let table = load_table(toml)?;
    let base_name = match table.get("base") {
        Some(v) => v.as_str().ok_or("base: expected string")?,
        None => DEFAULT_BASE,
    };
    let base = builtins
        .iter()
        .find(|t| t.name == base_name)
        .ok_or_else(|| format!("unknown base \"{base_name}\""))?;
    let mut theme = base.clone();
    theme.name = format!("{}-custom", base.name);
    apply(&table, &mut theme)?;
    Ok(theme)
}

// Built-in files carry every token and no base, so nothing is inherited.
fn parse_full(toml: &str) -> Result<Theme, String> {
    let table = load_table(toml)?;
    if table.contains_key("base") {
        return Err("unknown key \"base\"".into());
    }
    if !table.contains_key("name") {
        return Err("missing key \"name\"".into());
    }
    for (section, keys) in [("colors", &COLOR_KEYS[..]), ("ansi", &ANSI_KEYS[..])] {
        let sub = table.get(section).and_then(Value::as_table);
        for key in keys {
            if !sub.is_some_and(|t| t.contains_key(*key)) {
                return Err(format!("missing key \"{section}.{key}\""));
            }
        }
    }
    let mut theme = Theme {
        name: String::new(),
        background: 0,
        surface: 0,
        border: 0,
        text: 0,
        muted: 0,
        accent_app: 0,
        accent_command: 0,
        accent_extension: 0,
        ansi: [0; 16],
    };
    apply(&table, &mut theme)?;
    Ok(theme)
}

fn load_table(toml: &str) -> Result<Table, String> {
    toml.parse::<Table>().map_err(|e| e.to_string())
}

fn apply(table: &Table, theme: &mut Theme) -> Result<(), String> {
    for (key, value) in table {
        match key.as_str() {
            "name" => {
                theme.name = value.as_str().ok_or("name: expected string")?.to_string();
            }
            "base" => {}
            "colors" => {
                for (k, v) in section(value, "colors")? {
                    let slot = color_slot(theme, k).ok_or_else(|| unknown(k))?;
                    *slot = hex(v, "colors", k)?;
                }
            }
            "ansi" => {
                for (k, v) in section(value, "ansi")? {
                    let i = ANSI_KEYS.iter().position(|n| n == k).ok_or_else(|| unknown(k))?;
                    theme.ansi[i] = hex(v, "ansi", k)?;
                }
            }
            other => return Err(unknown(other)),
        }
    }
    Ok(())
}

fn section<'a>(value: &'a Value, name: &str) -> Result<&'a Table, String> {
    value.as_table().ok_or_else(|| format!("{name}: expected table"))
}

fn color_slot<'a>(theme: &'a mut Theme, key: &str) -> Option<&'a mut u32> {
    Some(match key {
        "background" => &mut theme.background,
        "surface" => &mut theme.surface,
        "border" => &mut theme.border,
        "text" => &mut theme.text,
        "muted" => &mut theme.muted,
        "accent_app" => &mut theme.accent_app,
        "accent_command" => &mut theme.accent_command,
        "accent_extension" => &mut theme.accent_extension,
        _ => return None,
    })
}

fn unknown(key: &str) -> String {
    format!("unknown key \"{key}\"")
}

fn hex(value: &Value, section: &str, key: &str) -> Result<u32, String> {
    let err = || format!("{section}.{key}: expected #RRGGBB");
    let digits = value.as_str().and_then(|s| s.strip_prefix('#')).ok_or_else(err)?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(err());
    }
    u32::from_str_radix(digits, 16).map_err(|_| err())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtins() -> Vec<Theme> {
        vec![nebula_dark(), nebula_light()]
    }

    #[test]
    fn builtins_load() {
        let dark = nebula_dark();
        assert_eq!(dark.name, "nebula-dark");
        assert_eq!(dark.background, 0x15141B);
        assert_eq!(dark.ansi[1], 0xFF8A8A);
        assert_eq!(dark.ansi[15], 0xECEAF2);
        let light = nebula_light();
        assert_eq!(light.name, "nebula-light");
        assert_eq!(light.background, 0xF0EFF4);
        assert_eq!(light.ansi[0], 0x15141B);
    }

    #[test]
    fn light_base_override_keeps_other_tokens() {
        let src = "name = \"mine\"\nbase = \"nebula-light\"\n[colors]\ntext = \"#000000\"\n";
        let t = parse(src, &builtins()).unwrap();
        let light = nebula_light();
        assert_eq!(t.name, "mine");
        assert_eq!(t.text, 0x000000);
        assert_eq!(t.background, light.background);
        assert_eq!(t.surface, light.surface);
        assert_eq!(t.border, light.border);
        assert_eq!(t.muted, light.muted);
        assert_eq!(t.accent_app, light.accent_app);
        assert_eq!(t.accent_command, light.accent_command);
        assert_eq!(t.accent_extension, light.accent_extension);
        assert_eq!(t.ansi, light.ansi);
    }

    #[test]
    fn base_defaults_to_dark_and_name_to_custom() {
        let t = parse("[ansi]\nred = \"#112233\"\n", &builtins()).unwrap();
        let dark = nebula_dark();
        assert_eq!(t.name, "nebula-dark-custom");
        assert_eq!(t.ansi[1], 0x112233);
        assert_eq!(t.background, dark.background);
    }

    #[test]
    fn invalid_hex_names_key() {
        let err = parse("[colors]\ntext = \"#12345\"\n", &builtins()).unwrap_err();
        assert_eq!(err, "colors.text: expected #RRGGBB");
        let err = parse("[ansi]\nred = \"FF8A8A\"\n", &builtins()).unwrap_err();
        assert_eq!(err, "ansi.red: expected #RRGGBB");
        let err = parse("[colors]\ntext = \"#GGGGGG\"\n", &builtins()).unwrap_err();
        assert_eq!(err, "colors.text: expected #RRGGBB");
    }

    #[test]
    fn unknown_base_errors() {
        let err = parse("base = \"x\"\n", &builtins()).unwrap_err();
        assert_eq!(err, "unknown base \"x\"");
    }

    #[test]
    fn unknown_keys_error() {
        assert_eq!(parse("foo = 1\n", &builtins()).unwrap_err(), "unknown key \"foo\"");
        assert_eq!(
            parse("[colors]\nfoo = \"#000000\"\n", &builtins()).unwrap_err(),
            "unknown key \"foo\""
        );
        assert_eq!(
            parse("[ansi]\norange = \"#000000\"\n", &builtins()).unwrap_err(),
            "unknown key \"orange\""
        );
    }

    #[test]
    fn full_parse_requires_every_token() {
        let err = parse_full("name = \"x\"\n[colors]\ntext = \"#000000\"\n").unwrap_err();
        assert_eq!(err, "missing key \"colors.background\"");
    }
}
