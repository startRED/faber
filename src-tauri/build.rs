fn main() {
    // The screen is embedded from ../dist, built by `npm run build`. A fresh checkout gets an empty folder so the
    // Rust side still compiles and tests on its own.
    std::fs::create_dir_all("../dist").expect("create ../dist");
    tauri_build::build()
}
