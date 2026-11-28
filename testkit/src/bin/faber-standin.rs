//! The stand-in CLI of the real-work benchmark's other side (docs/superpowers/plans/2026-10-22-real-work-bench.md): copied
//! as `claude.exe` into a temporary PATH, it reads its prompt from stdin to the end, prints `reply.txt` from its own folder
//! as plain text and exits 0, the way `claude -p --output-format text` answers. It never reads or writes anything else.

use std::io::{Read, Write};

fn main() -> std::process::ExitCode {
    let mut prompt = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut prompt);
    let reply = std::env::current_exe().ok().and_then(|exe| std::fs::read(exe.with_file_name("reply.txt")).ok());
    let Some(reply) = reply else {
        eprintln!("faber-standin: no reply.txt beside the program");
        return std::process::ExitCode::FAILURE;
    };
    let mut out = std::io::stdout().lock();
    if out.write_all(&reply).and_then(|()| out.flush()).is_err() {
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
