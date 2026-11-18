use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    dirs::config_dir().expect("%APPDATA%").join("nebucat")
}

pub fn data_dir() -> PathBuf {
    dirs::data_local_dir().expect("%LOCALAPPDATA%").join("nebucat")
}

pub fn init_logging() {
    let dir = data_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(file) = std::fs::File::create(dir.join("nebucat.log")) {
        let _ = simplelog::WriteLogger::init(
            simplelog::LevelFilter::Info,
            simplelog::Config::default(),
            file,
        );
    }
}
