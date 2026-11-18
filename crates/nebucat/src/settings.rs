use nebucat_core::config::{self, Config};

use crate::paths;

/// Creates the config folder layout and writes the default config.toml if missing.
pub fn ensure_layout() {
    let dir = paths::config_dir();
    for sub in ["themes", "plugins"] {
        let _ = std::fs::create_dir_all(dir.join(sub));
    }
    let file = dir.join("config.toml");
    if !file.exists() {
        if let Err(e) = std::fs::write(&file, config::DEFAULT_FILE) {
            log::error!("write default config: {e}");
        }
    }
}

pub fn load() -> Result<Config, String> {
    let text = std::fs::read_to_string(paths::config_dir().join("config.toml")).unwrap_or_default();
    config::parse(&text)
}
