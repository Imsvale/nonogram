fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/n.ico");
        if let Err(e) = res.compile() {
            // rc.exe / windres not available in this environment (e.g. rust-analyzer).
            // Icon embedding is skipped; the binary still compiles correctly.
            println!("cargo:warning=Icon embedding skipped: {e}");
        }
    }
}
