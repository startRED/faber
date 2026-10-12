//! Stands in for a CLI binary: replays the recording named by FABER_FAKE_RECORDING (see `faber_testkit::fake`).

use faber_testkit::fake::{Env, run};
use std::io;

fn main() {
    if let Some(code) = faber_testkit::gate::guard_mode() {
        std::process::exit(code);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [mode, sandbox] = args.as_slice()
        && matches!(
            mode.as_str(),
            "--project-git" | "--project-git-delete" | "--project-git-race" | "--project-path-race" | "--project-git-clean"
        )
    {
        let sandbox = std::path::Path::new(sandbox);
        let result = match mode.as_str() {
            "--project-git-delete" => faber_testkit::screen::projects::probe_deletion(sandbox),
            "--project-git-race" => faber_testkit::screen::projects::probe_race(sandbox),
            "--project-path-race" => faber_testkit::screen::projects::probe_path_race(sandbox),
            "--project-git-clean" => faber_testkit::screen::projects::probe_cleanup(sandbox),
            _ => faber_testkit::screen::projects::probe(sandbox),
        };
        match result {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                eprintln!("project git: {error}");
                std::process::exit(1);
            }
        }
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
