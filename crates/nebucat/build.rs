fn main() {
    println!("cargo:rerun-if-changed=../../assets/icon/nebucat.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        #[cfg(windows)]
        winresource::WindowsResource::new()
            .set_icon("../../assets/icon/nebucat.ico")
            .compile()
            .expect("embed icon");
    }
}
