//! The runner: `cargo t` (see `.cargo/config.toml`). All of it is in `faber_testkit::runner`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(faber_testkit::runner::run(&args));
}
