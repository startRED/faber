//! A project with a real git repository on the isolated desktop, for the scenarios that need one.
use super::{Result, start_settings};
use crate::app::Ctx;
use crate::sandbox::Launch;
use std::path::Path;

pub(super) fn fixture(ctx: &mut Ctx) -> Result<Launch> {
    start_settings(ctx, "en")?;
    super::projects::run_probe(ctx, "--project-screen-fixture")?;
    let git = faber_engine::cli::find_cli("git").ok_or("git is unavailable")?;
    Ok(Launch { git_bin: Some(Path::new(&git.command).parent().ok_or("git has no directory")?.into()), ..Default::default() })
}
