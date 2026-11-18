use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub hotkey: String,
    pub theme: String,
    pub autostart: bool,
    pub max_results: usize,
    pub command: CommandCfg,
    pub web: WebCfg,
    pub files: FilesCfg,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "Alt+Space".into(),
            theme: "system".into(),
            autostart: true,
            max_results: 8,
            command: CommandCfg::default(),
            web: WebCfg::default(),
            files: FilesCfg::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    Cmd,
    Powershell,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CommandCfg {
    pub shell: Shell,
    pub timeout_secs: u64,
}

impl Default for CommandCfg {
    fn default() -> Self {
        Self { shell: Shell::Cmd, timeout_secs: 15 }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebCfg {
    pub enabled: bool,
    pub url: String,
}

impl Default for WebCfg {
    fn default() -> Self {
        Self {
            enabled: false,
            url: "https://www.google.com/search?q={query}".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FilesCfg {
    pub max_results: usize,
}

impl Default for FilesCfg {
    fn default() -> Self {
        Self { max_results: 8 }
    }
}

impl Config {
    pub fn parse(s: &str) -> Result<Config, String> {
        parse(s)
    }
}

pub fn parse(s: &str) -> Result<Config, String> {
    let mut cfg: Config = toml::from_str(s).map_err(|e| toml_error(s, &e))?;
    cfg.max_results = cfg.max_results.clamp(1, 9);
    Ok(cfg)
}

/// One-line `line N: message` form, because errors are shown in a one-line banner.
pub(crate) fn toml_error(src: &str, e: &toml::de::Error) -> String {
    let msg = e.message().trim();
    match e.span() {
        Some(span) => {
            let line = src[..span.start.min(src.len())].matches('\n').count() + 1;
            format!("line {line}: {msg}")
        }
        None => msg.to_string(),
    }
}

pub const DEFAULT_FILE: &str = r##"# nebucat settings. Saved changes apply without a restart.

# Global shortcut: Ctrl, Alt, Shift or Win plus one key (A-Z, 0-9, Space, F1-F12).
hotkey = "Alt+Space"

# "system" follows Windows, "nebula-dark", "nebula-light", or a file name from themes\ without .toml.
theme = "system"

# true starts nebucat when you sign in to Windows; false does not.
autostart = true

# Result rows shown, 1 to 9.
max_results = 8

[command]
# Shell for > commands: "cmd" or "powershell".
shell = "cmd"
# Seconds before a running command is stopped.
timeout_secs = 15

[web]
# true adds a web search row when no app matches.
enabled = false
# Search URL; {query} is replaced with the typed text.
url = "https://www.google.com/search?q={query}"

[files]
# File rows shown for / searches.
max_results = 8
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_default() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn default_file_is_default() {
        assert_eq!(Config::parse(DEFAULT_FILE).unwrap(), Config::default());
    }

    #[test]
    fn unknown_key_errors() {
        assert!(Config::parse("foo = 1").is_err());
        assert!(Config::parse("[web]\nfoo = 1").is_err());
    }

    #[test]
    fn max_results_clamped() {
        assert_eq!(Config::parse("max_results = 0").unwrap().max_results, 1);
        assert_eq!(Config::parse("max_results = 50").unwrap().max_results, 9);
    }

    #[test]
    fn shell_powershell() {
        let c = Config::parse("[command]\nshell = \"powershell\"").unwrap();
        assert_eq!(c.command.shell, Shell::Powershell);
        assert_eq!(c.command.timeout_secs, 15);
    }

    #[test]
    fn error_is_one_line_with_line_number() {
        let e = Config::parse("theme = \"system\"\nhotkey = ").unwrap_err();
        assert!(e.starts_with("line 2: "), "{e}");
        assert!(!e.contains('\n'), "{e}");
        let e = Config::parse("nope = 1").unwrap_err();
        assert!(e.starts_with("line 1: ") && e.contains("nope"), "{e}");
    }
}
