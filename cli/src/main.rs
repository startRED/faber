//! Faber's command line.
mod args;
mod connect;
mod mcp;
mod out;
mod verbs;
mod words;
use args::Args;
use out::Out;
fn main() {
    std::process::exit(run());
}
fn run() -> i32 {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let flags: Vec<&String> = raw.iter().take_while(|s| *s != "--").collect();
    let out = Out::new(
        flags.iter().any(|s| s.as_str() == "--json"),
        flags.iter().any(|s| s.as_str() == "--human"),
        words::Words::new(&words::language(None)),
    );
    let args = match Args::parse(raw) {
        Ok(args) => args,
        Err(_) => return out.fail("cli.error.usage", out::USAGE),
    };
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
        return out.fail("cli.error.channel", out::UNREACHED);
    };
    if args.word(0) == Some("mcp") && !args.on("help") {
        if args.words.len() != 1 || !args.only(&[]) {
            return out.fail("cli.error.usage", out::USAGE);
        }
        return runtime.block_on(mcp::run(&args));
    }
    runtime.block_on(async {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => out::STOPPED,
            result = verbs::run(args, out) => result,
        }
    })
}
