//! A project with a real git repository on the isolated desktop, for the scenarios that need one.
use super::projects::git;
use super::{Result, start_settings};
use crate::app::Ctx;
use crate::sandbox::Launch;
use std::path::Path;

/// Settings done, and one project, "Demo", whose folder is a git repository with a single commit.
pub(super) fn fixture(ctx: &mut Ctx) -> Result<Launch> {
    start_settings(ctx, "en")?;
    let git_cli = faber_engine::cli::find_cli("git").ok_or("git is unavailable")?;
    let program = &git_cli.command;
    let repo = ctx.sb.sandbox.join("repo");
    std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    git(program, &repo, &["init", "--initial-branch=owner"])?;
    git(program, &repo, &["config", "user.name", "Fixture"])?;
    git(program, &repo, &["config", "user.email", "fixture@example.invalid"])?;
    std::fs::write(repo.join("fixture"), "original").map_err(|e| e.to_string())?;
    git(program, &repo, &["add", "--", "fixture"])?;
    git(program, &repo, &["commit", "--no-gpg-sign", "-m", "fixture"])?;
    let project = faber_engine::model::Project::new("demo".into(), "Demo".into(), repo.to_string_lossy().into());
    std::fs::write(ctx.sb.data.join("projects.json"), serde_json::to_vec(&[project]).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(Launch { git_bin: Some(Path::new(program).parent().ok_or("git has no directory")?.into()), ..Default::default() })
}
