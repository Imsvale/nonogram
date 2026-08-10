fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    println!("cargo:rerun-if-changed=assets/n.ico");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir      = std::env::var("OUT_DIR").unwrap();
    // Forward slashes so the .rc parser handles the path correctly on all hosts
    let ico_path = format!("{manifest_dir}/assets/n.ico").replace('\\', "/");
    let rc_path  = format!("{out_dir}/app.rc");
    let res_path = format!("{out_dir}/app.res");

    std::fs::write(&rc_path, format!("1 ICON \"{ico_path}\"\n")).unwrap();

    // MSVC toolchain: prefer rc.exe (Windows SDK), fall back to llvm-rc (LLVM).
    // GNU toolchain:  prefer llvm-rc, fall back to windres (produces COFF .res).
    let is_msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let compiled = if is_msvc {
        try_rc("rc.exe",  &["/nologo", "/fo", &res_path, &rc_path])
        || try_rc("llvm-rc", &["/fo",  &res_path, &rc_path])
    } else {
        try_rc("llvm-rc", &["/fo",  &res_path, &rc_path])
        || try_rc("windres", &["-i", &rc_path, "-o", &res_path, "--output-format=coff"])
    };

    if compiled {
        println!("cargo:rustc-link-arg={res_path}");
    } else {
        println!("cargo:warning=Icon embedding skipped: install Windows SDK (rc.exe) or LLVM (llvm-rc)");
    }
}

fn try_rc(tool: &str, args: &[&str]) -> bool {
    std::process::Command::new(tool)
        .args(args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
