//! The runner: `cargo t` (see `.cargo/config.toml`). All of it is in `faber_testkit::runner`.

fn main() {
    if let Some(code) = faber_engine::proc::guard_mode() {
        std::process::exit(code);
    }
    // Below normal: whatever the person is doing (a game, a call) wins the CPU. Everything the runner starts (cargo,
    // rustc, the linker, the test programs, the hidden app) inherits this class.
    #[cfg(windows)]
    let _ = winsafe::HPROCESS::GetCurrentProcess().SetPriorityClass(winsafe::co::PRIORITY_CLASS::BELOW_NORMAL);
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(faber_testkit::runner::run(&args));
}
