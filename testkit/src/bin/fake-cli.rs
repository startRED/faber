//! Stands in for a CLI binary: replays the recording named by FABER_FAKE_RECORDING (see `faber_testkit::fake`).

use faber_testkit::fake::{Env, run};
use std::io;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--faber-process-guard")
        && let Some(i) = args.iter().position(|a| a == "--held-guard")
    {
        for _ in 0..200 {
            if std::path::Path::new(&args[i + 1]).exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
    if let Some(code) = faber_engine::proc::guard_mode() {
        std::process::exit(code);
    }
    if let Some(code) = faber_testkit::gate::run(&args) {
        std::process::exit(code);
    }
    let Some(recording) = std::env::var_os("FABER_FAKE_RECORDING") else {
        eprintln!("fake-cli: set FABER_FAKE_RECORDING to a recording under tests/fixtures");
        std::process::exit(2);
    };
    let env = Env { recording: recording.into(), state_dir: std::env::var_os("FABER_FAKE_STATE_DIR").map(Into::into) };
    let code = match run(&env, &args, &mut io::stdin().lock(), &mut io::stdout().lock(), &mut io::stderr().lock()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fake-cli: {e}");
            1
        }
    };
    std::process::exit(code);
}
