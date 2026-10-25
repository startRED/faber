//! Stands in for a CLI binary: replays the recording named by FABER_FAKE_RECORDING (see `faber_testkit::fake`).

use faber_testkit::fake::{Env, run};
use std::io;

fn replay_arguments(args: &[String]) -> io::Result<(Option<&str>, &[String])> {
    if args.first().map(String::as_str) != Some("--faber-recording") {
        return Ok((None, args));
    }
    let path = args.get(1).filter(|path| !path.is_empty()).ok_or_else(|| io::Error::other("--faber-recording requires a fixture path"))?;
    Ok((Some(path), &args[2..]))
}

fn main() {
    if let Some(code) = faber_testkit::gate::guard_mode() {
        std::process::exit(code);
    }
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let (recording, args) = match replay_arguments(&arguments) {
        Ok(arguments) => arguments,
        Err(error) => {
            eprintln!("fake-cli: {error}");
            std::process::exit(2);
        }
    };
    if let [mode, sandbox] = args
        && mode == "--project-git"
    {
        match faber_testkit::screen::projects::probe(std::path::Path::new(sandbox)) {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                eprintln!("project git: {error}");
                std::process::exit(1);
            }
        }
    }
    if let Some(code) = faber_testkit::gate::run(args) {
        std::process::exit(code);
    }
    let Some(recording) = recording.map(std::path::PathBuf::from).or_else(|| std::env::var_os("FABER_FAKE_RECORDING").map(Into::into))
    else {
        eprintln!("fake-cli: set FABER_FAKE_RECORDING to a recording under tests/fixtures");
        std::process::exit(2);
    };
    let env = Env { recording, state_dir: std::env::var_os("FABER_FAKE_STATE_DIR").map(Into::into) };
    let code = match run(&env, args, &mut io::stdin().lock(), &mut io::stdout().lock(), &mut io::stderr().lock()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("fake-cli: {e}");
            1
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_override_is_removed_before_the_real_protocol_arguments() {
        let args = vec!["--faber-recording".into(), "fixture with spaces.jsonl".into(), "app-server".into()];
        let (recording, rest) = replay_arguments(&args).unwrap();
        assert_eq!(recording, Some("fixture with spaces.jsonl"));
        assert_eq!(rest, &["app-server"]);
        let args = vec!["auth".into(), "status".into()];
        assert_eq!(replay_arguments(&args).unwrap(), (None, args.as_slice()));
        assert!(replay_arguments(&["--faber-recording".into()]).is_err());
        assert!(replay_arguments(&["--faber-recording".into(), String::new()]).is_err());
    }
}
