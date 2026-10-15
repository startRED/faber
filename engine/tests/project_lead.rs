use faber_engine::project::lead::accept;
use faber_engine::project::{Contract, Plan, Task, TaskKind};
#[test]
fn malformed_generic_and_uncompiled_plans_never_dispatch() {
    let p = Plan::new("m1", 1, vec![Task::new("a", "Do work", TaskKind::Research)]);
    assert!(accept(p.clone(), &[], |_| Ok(())).is_err());
    let mut p = p;
    p.tasks[0].checks = vec!["C1".into()];
    p.tasks[0].paths = vec!["one.rs".into()];
    assert!(accept(p.clone(), &[], |_| Err("reason.bar.missing")).is_err());
    let c = Contract::new("c1", "lead", &"1".repeat(40)).seal();
    p.tasks[0].contracts = vec![c.pin()];
    p.compile_receipt = Some("compile-1".into());
    assert!(accept(p.clone(), std::slice::from_ref(&c), |_| Ok(())).is_ok());
    let mut changed = c;
    changed.version = 2;
    changed = changed.seal();
    assert!(accept(p, &[changed], |_| Ok(())).is_err());
}
