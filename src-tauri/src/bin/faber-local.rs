//! Independent development rollback supervisor. It never starts a GUI runtime or a console window.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    if let Some(code) = faber_engine::proc::guard_mode() {
        std::process::exit(code);
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = (|| {
        let recovering = match args.as_slice() {
            [arg] if arg == "--transition" => false,
            [arg] if arg == "--recover" => true,
            _ => return Err(()),
        };
        let dev = faber::local_update::Development::from_environment().map_err(|_| ())?;
        faber::local_update::helper_main(&dev, recovering).map_err(|_| ())
    })();
    if result.is_err() {
        std::process::exit(1);
    }
}
