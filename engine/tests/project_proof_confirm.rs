use faber_engine::model::Project;
use faber_engine::project::mission::{confirm, confirmed, view};

fn text(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The owner is shown declaration A; the file becomes B while the sheet is open. Confirming A must not record B.
#[test]
fn a_confirmation_of_a_list_that_is_no_longer_declared_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "## Proof commands\n```\nnpm test\n```").unwrap();
    let project = Project::new("p1".into(), "P".into(), dir.path().to_string_lossy().into_owned());
    let shown = view(&project);
    assert_eq!(shown.declared, ["npm test"]);

    std::fs::write(dir.path().join("AGENTS.md"), "## Proof commands\n```\nnpm test\nnpm run e2e\n```").unwrap();
    assert_eq!(confirm(&project, &shown.commands, &shown.setup, &shown.declared), Err("reason.proof.declaredChanged"));
    assert_eq!(confirmed(&project), Err("reason.proof.unconfirmed"), "nothing was saved");

    // Shown again, the new list is confirmed and holds.
    let again = view(&project);
    assert_eq!(again.declared, ["npm test", "npm run e2e"]);
    let saved = confirm(&project, &again.commands, &again.setup, &again.declared).unwrap();
    assert_eq!(confirmed(&saved).unwrap(), ["npm test", "npm run e2e"]);

    // An edited list still confirms, as long as the declaration it came from is the one shown.
    let edited = confirm(&project, &text(&["npm run lint"]), &again.setup, &again.declared).unwrap();
    assert_eq!(confirmed(&edited).unwrap(), ["npm run lint"]);
}
