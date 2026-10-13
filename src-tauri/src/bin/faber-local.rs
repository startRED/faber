//! Independent development rollback supervisor. It never starts a GUI runtime or a console window.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    if let Some(code) = faber_engine::proc::guard_mode() {
        std::process::exit(code);
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = (|| {
        let dev = if std::env::var_os("FABER_CHANNEL").is_some() {
            faber::local_update::Development::from_environment()
        } else {
            faber::local_update::Development::from_launcher(&std::env::current_exe().map_err(|_| ())?)
        }
        .map_err(|_| ())?;
        #[cfg(feature = "test-hooks")]
        if args.as_slice() == ["--test-autostart-command"] {
            let manifest = dev.manager().map_err(|_| ())?.manifest().map_err(|_| ())?.ok_or(())?;
            let command = faber::autostart::launch_command(&dev.binary(manifest.active_slot), Some(&dev)).map_err(|_| ())?;
            return std::fs::write(
                dev.data().join("test-autostart-command.json"),
                serde_json::to_vec(&serde_json::json!({"program": command.program, "args": command.args})).map_err(|_| ())?,
            )
            .map_err(|_| ());
        }
        #[cfg(feature = "test-hooks")]
        if args.as_slice() == ["--test-compatible"] {
            return faber::local_update::native_compatible(
                &dev.binary(faber_engine::project::update::Slot::A),
                &dev.binary(faber_engine::project::update::Slot::B),
                &dev.data(),
            )
            .map_err(|_| ());
        }
        #[cfg(feature = "test-hooks")]
        if args.as_slice() == ["--test-transition"] {
            let manager = dev.manager().map_err(|_| ())?;
            let manifest = manager.manifest().map_err(|_| ())?.ok_or(())?;
            let repo = faber_engine::project::RepoIdentity::new("repo", "checkout");
            let mut grant = faber_engine::project::Grant::new("repo", faber_engine::project::GitMode::Seguro, 1, 1);
            grant.self_update = true;
            let payload = std::fs::read(dev.binary(manifest.active_slot.other())).map_err(|_| ())?;
            return dev
                .transition(
                    &grant,
                    &repo,
                    1,
                    faber_engine::project::update::SwapCheck { payload: &payload, active_window: false, active_operations: false },
                    faber::local_update::now(),
                )
                .map_err(|_| ());
        }
        match args.as_slice() {
            [arg] if arg == "--transition" => faber::local_update::helper_main(&dev, false),
            [arg] if arg == "--recover" => faber::local_update::helper_main(&dev, true),
            [arg] if arg == "--launch" || arg == "--open" => faber::local_update::launch(&dev, arg == "--open"),
            _ => return Err(()),
        }
        .map_err(|_| ())
    })();
    if result.is_err() {
        std::process::exit(1);
    }
}
