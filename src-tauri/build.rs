mod chime;

fn main() {
    println!("cargo:rerun-if-changed=chime.rs");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(out.join("attention.wav"), chime::wav()).expect("write Faber's chime");
    // The screen is embedded from ../dist, built by `npm run build`. A fresh checkout gets an empty folder so the
    // Rust side still compiles and tests on its own.
    std::fs::create_dir_all("../dist").expect("create ../dist");
    tauri_build::build();
    // tauri-build links its Windows resource (the manifest asking for Common Controls v6) into binaries only; the
    // spike host is an example, and without it Windows refuses to start it.
    if std::env::var_os("CARGO_FEATURE_PANE_SPIKE").is_some() && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-examples={}", out.join("resource.lib").display());
    }
}
