//! `faber`: Faber on the command line. With no verb at a terminal it opens the terminal screen; with a verb it does one
//! thing and prints words for a person or, with `--json`, JSON for a program. It talks to the running Faber over its
//! local channel (`faber-channel`) and starts Faber hidden in the tray when nothing answers.

mod args;
mod chat;
mod connect;
mod mcp;
mod out;
mod rpc;
mod screen;
mod thread;
mod verbs;
mod words;

use args::Args;
use out::{Out, Tone, USAGE};
use words::Words;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let system = Words::new(&words::language(None));
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(flag) => {
            let out = Out::new(false, false, system);
            eprintln!("{}", out.paint(Tone::Bad, &out.words.fill("cli.error.flag", &[("flag", &flag)])));
            return USAGE;
        }
    };
    let out = Out::new(args.on("json"), args.on("plain"), system);
    if args.on("version") {
        out.line(&format!("faber {}", env!("CARGO_PKG_VERSION")));
        return out::DONE;
    }
    if args.on("help") || args.word(0) == Some("help") {
        verbs::help(&out);
        return out::DONE;
    }
    // One thread is enough for a client: it waits on the channel and on the terminal, never on the processor.
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
        return out.fail("cli.error.channel", out::UNREACHED);
    };
    runtime.block_on(async move {
        match args.word(0) {
            None if out.interactive => screen::run(args, out).await,
            None => {
                verbs::help(&out);
                USAGE
            }
            Some(_) => verbs::run(args, out).await,
        }
    })
}
