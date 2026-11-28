//! Paired project turns with one engine part disabled in the second condition.
use super::*;
use pure::{Condition, PartRow, part_order};

pub(super) async fn run(mut options: Options) -> Result<(), Failure> {
    let [author] = options.vendors.as_slice() else { return Err("--parts needs one --vendors value for both trials".into()) };
    let author = *author;
    let vendor = if author == Vendor::Claude { Vendor::Codex } else { Vendor::Claude };
    let profile = strongest_judge(vendor);
    options.judge_vendor = vendor;
    if options.judge_model.is_empty() {
        options.judge_model = profile.model.into();
    }
    if options.judge_effort.is_empty() {
        options.judge_effort = profile.effort.into();
    }
    let source = repo_root().join("bench");
    let mut requests = load_requests(&source, &options.only)?;
    requests.retain(|request| request.kind == Kind::Project);
    if requests.is_empty() || options.only.iter().any(|id| !requests.iter().any(|request| request.id == *id)) {
        return Err("--parts requires project request IDs".into());
    }
    let plan = || options.parts.iter().copied().flat_map(|part| requests.iter().map(move |request| (part, request))).enumerate();
    let pairs = options.parts.len() * requests.len();
    let nominal = pairs as f64 * 3.0 * options.per_run_usd;
    println!(
        "{pairs} pairs: {} Faber trials on {}, {pairs} blind judgments on {} ({} {})",
        2 * pairs,
        author.name(),
        vendor.name(),
        options.judge_model,
        options.judge_effort
    );
    for (index, (part, request)) in plan() {
        let [first, second] = part_order(&options.run_id, index).map(Condition::name);
        println!("  {} / {}: {first} then {second}", part.name(), request.id);
    }
    println!(
        "Allowance-based spend estimate: US${nominal:.2} before the run cap; admission cap US${:.2} or {} first-time tokens",
        options.cap.max_usd, options.cap.max_fresh_tokens
    );
    if options.dry_run {
        return Ok(());
    }
    if faber_engine::signals::claude_env_billed(std::env::vars_os().map(|(name, _)| name)) {
        return Err("an environment variable would bill Claude Code per use; the benchmark runs on subscriptions only".into());
    }
    let node = find_cli("node").ok_or("node is not on PATH")?;
    let cli = |vendor: Vendor| find_cli(vendor.name()).ok_or_else(|| format!("{} is not installed", vendor.name()));
    let clis = BTreeMap::from([(author, cli(author)?)]);
    let judges = BTreeMap::from([(author, clis[&author].clone()), (vendor, cli(vendor)?)]);
    let root = std::path::absolute(options.dir.join(&options.run_id))?;
    std::fs::create_dir_all(&options.dir)?;
    std::fs::create_dir(&root)?;
    let inputs = inputs::snapshot(&source, &root)?;
    let judge_dir = root.join("judge");
    std::fs::create_dir(&judge_dir)?;
    let mut accounts = bench_quiet::Accounts::new(&root);
    accounts.busy = options.busy_account;
    let mut spent = Spend::default();
    let mut rows: Vec<_> = plan().map(|(index, (part, request))| PartRow::new(part, &request.id, &options.run_id, index)).collect();
    let save = |rows: &[PartRow], spent: &Spend| -> Result<(), Failure> {
        let results = json!({ "runId": options.run_id, "author": author.name(), "judge": vendor.name(),
            "judgeModel": options.judge_model, "judgeEffort": options.judge_effort, "spent": spent, "rows": rows });
        std::fs::write(root.join("results.json"), serde_json::to_string_pretty(&results)?)?;
        std::fs::write(root.join("tables.md"), pure::render_parts(rows))?;
        Ok(())
    };
    save(&rows, &spent)?;
    for (index, (part, request)) in plan() {
        let pair_root = root.join(part.name());
        for condition in rows[index].order {
            if let Some(why) = options.cap.reached(&spent) {
                rows[index].note = Some(why);
                break;
            }
            let trial_root = pair_root.join(condition.name());
            let work = prepare(request, &trial_root, &node.command, &inputs, None)?;
            if let Err(error) = accounts.wait(&judges, &[author], &mut spent).await {
                rows[index].note = Some(error.to_string());
                save(&rows, &spent)?;
                return Err(error.into());
            }
            let mut side = run_faber(
                &clis,
                request,
                &work,
                &trial_root,
                options.cap.allowance(&spent, options.per_run_usd),
                false,
                None,
                Some((condition == Condition::Off).then_some(part)),
            )
            .await;
            accounts.worked(&[author]);
            spent.add(&side.usage, side.cost_usd);
            *rows[index].side_mut(condition) = Some(side.clone());
            save(&rows, &spent)?;
            if side.folder.is_some() {
                prove(&node, request, &work, &mut side, &trial_root, &inputs).await;
                *rows[index].side_mut(condition) = Some(side);
                save(&rows, &spent)?;
            }
        }
        let row = &mut rows[index];
        let [a, b] = [row.a, row.a.other()].map(|condition| row.side(condition));
        if let Some(why) = options.cap.reached(&spent) {
            row.note = Some(why);
        } else if let (Some(a), Some(b)) = (a, b)
            && !a.reply.trim().is_empty()
            && !b.reply.trim().is_empty()
        {
            if let Err(error) = accounts.wait(&judges, &[vendor], &mut spent).await {
                row.note = Some(error.to_string());
                save(&rows, &spent)?;
                return Err(error.into());
            }
            let independent = a.authors.independent_of(vendor.name()) && b.authors.independent_of(vendor.name());
            let judged =
                judge(&options, &judges[&vendor], request, a, b, &judge_dir, options.cap.allowance(&spent, options.per_run_usd)).await;
            accounts.worked(&[vendor]);
            spent.add(&judged.usage, judged.cost_usd);
            row.independent = independent;
            row.judge_cost_usd = judged.cost_usd;
            row.judge_usage = judged.usage;
            row.verdict = judged.verdict;
            row.note = judged.note;
        } else {
            row.note = Some("a side gave no reply".into());
        }
        save(&rows, &spent)?;
    }
    println!("{}\nresults: {}", pure::render_parts(&rows), root.join("results.json").display());
    Ok(())
}
