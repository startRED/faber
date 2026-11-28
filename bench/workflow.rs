//! A small, opt-in pair through Faber with the same model and starting project on each side.

use super::*;
use faber_engine::{
    budget::{Billing, Reach},
    events::EngineEvent,
    router::Effort,
    store::{RealDisk, replace},
};

const REQUEST: &str = "fix-invoice";
const MODEL: &str = "gpt-6.2-sol";
const CONSTRAINTS: &str = "Work in this disposable project without delegation, external tools or publishing. Use commands and edits inside the project folder without requesting elevated execution. Report the exact error if local tools prevent completion.";
const GUIDANCE: &str = "Open and use Faber's workflow-proof skill for this task. Report the focused evidence you obtained.";
const OBSERVATION: &str = "observation.json";

pub(super) fn account_is_safe(events: &[EngineEvent]) -> bool {
    let mut subscribed = false;
    let mut credits_unavailable = false;
    for event in events {
        match event {
            EngineEvent::Quota { used, .. } if *used >= 1.0 => return false,
            EngineEvent::Signals(signal) => {
                if signal.billing.is_some_and(|b| b != Billing::Subscription)
                    || signal.paid_fallback == Some(true)
                    || signal.overage_active == Some(true)
                    || signal.credit_balance.is_some_and(|v| v > 0.0)
                    || matches!(signal.reach, Some(Reach::Out(_)))
                {
                    return false;
                }
                subscribed |= signal.billing == Some(Billing::Subscription);
                credits_unavailable |= signal.paid_fallback == Some(false);
            }
            _ => {}
        }
    }
    subscribed && credits_unavailable
}

/// Prove the real guarded shell can edit its fixture and that the protected files exist and retain their bytes.
#[cfg(windows)]
pub(super) async fn check_local_command(cwd: &Path, data: &Path) -> Result<(), Failure> {
    let owned = tempfile::Builder::new().prefix("pilot-").tempdir_in(cwd)?;
    let peer = tempfile::Builder::new().prefix("pilot-").tempdir_in(cwd.parent().ok_or("invalid pilot project folder")?)?;
    let store = tempfile::Builder::new().prefix("pilot-").tempdir_in(data)?;
    for dir in [&owned, &peer, &store] {
        std::fs::write(dir.path().join("sample.txt"), "keep")?;
    }
    let quote = |path: &Path| format!("'{}'", path.to_string_lossy().replace('\'', "''"));
    let own = quote(&Path::new(owned.path().file_name().ok_or("invalid pilot fixture folder")?).join("sample.txt"));
    let peer_file = peer.path().join("sample.txt");
    let store_file = store.path().join("sample.txt");
    let input = json!({"command":format!(concat!(
        "$ErrorActionPreference = 'Stop'; Set-Content -LiteralPath {} -NoNewline edited; ",
        "try {{ Get-Content -LiteralPath {} }} catch {{ Write-Output STORE_READ_DENIED }}; ",
        "try {{ Set-Content -LiteralPath {} -NoNewline edited }} catch {{ Write-Output STORE_WRITE_DENIED }}; ",
        "try {{ Set-Content -LiteralPath {} -NoNewline edited }} catch {{ Write-Output PEER_WRITE_DENIED }}"
    ), own, quote(&store_file), quote(&store_file), quote(&peer_file))});
    let codex = find_cli("codex");
    let argv =
        faber_engine::permissions::guarded_command_argv(cwd, Permissions::AcceptEdits, "PowerShell", &input, codex.as_ref(), &find_cli)
            .ok_or("the guarded command profile is unavailable")?;
    let (program, argv) = argv.split_first().ok_or("no guarded command executable")?;
    let run = run_child(proc::marked(program, argv, cwd), program, None, Duration::from_secs(20)).await?;
    if run.timed_out
        || run.code != Some(0)
        || run.stdout.lines().collect::<Vec<_>>() != ["STORE_READ_DENIED", "STORE_WRITE_DENIED", "PEER_WRITE_DENIED"]
        || std::fs::read(owned.path().join("sample.txt"))? != b"edited"
        || std::fs::read(peer_file)? != b"keep"
        || std::fs::read(store_file)? != b"keep"
    {
        return Err(format!(
            "guarded command preflight failed (exit {:?}): {}",
            run.code,
            run.stderr.chars().take(300).collect::<String>()
        )
        .into());
    }
    Ok(())
}

/// Keep only the experiment's observations. Tool arguments and tool output stay in the isolated engine state.
pub(super) fn observe(root: &Path, choice: &Choice, replies: &[&Message]) {
    let tools: Vec<_> = replies.iter().flat_map(|m| m.tools.iter().flatten()).collect();
    let delegated = tools.iter().any(|tool| {
        let name = tool.name.to_ascii_lowercase();
        tool.parent.is_some()
            || name == "agent"
            || name.contains("subagent")
            || name.contains("spawn_agent")
            || name.contains("helper")
            || name.contains("conversation")
    });
    let profile_matched = !replies.is_empty()
        && replies.iter().all(|m| {
            m.vendor == Some(Vendor::Codex)
                && m.model == choice.model
                && m.asked_model.is_none()
                && m.effort.as_deref() == choice.effort.map(Effort::name)
        });
    let skill_read = tools.iter().any(|tool| {
        let input = tool.input.to_string().to_ascii_lowercase();
        tool.ok == Some(true) && input.contains("workflow-proof") && input.contains("skill.md")
    });
    let proof_refusals: u32 = replies.iter().map(|reply| u32::from(reply.proof_asks)).sum();
    let usage_reported = !replies.is_empty() && replies.iter().all(|m| m.input_tokens.is_some() && m.output_tokens.is_some());
    let value = json!({ "profileMatched": profile_matched, "delegationObserved": delegated,
        "skillReadObserved": skill_read, "tokenCountsReported": usage_reported, "engineReplies": replies.len(),
        "proofRefusals": proof_refusals });
    if let Err(error) = write_json(&root.join(OBSERVATION), &value) {
        eprintln!("could not save workflow observation: {error}");
    }
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), Failure> {
    replace(&RealDisk, path, &serde_json::to_vec_pretty(value)?)
        .map_err(|error| format!("could not save {}: {}", path.display(), error.key()))?;
    Ok(())
}

fn checkpoint(
    root: &Path,
    choice: &Choice,
    order: [&str; 2],
    sides: &BTreeMap<&str, serde_json::Value>,
    stopped: Option<&str>,
) -> Result<(), Failure> {
    write_json(
        &root.join("workflow-results.json"),
        &json!({ "request": REQUEST, "profile": choice, "executionOrder": order,
        "sides": sides, "stopped": stopped, "review": "blind-review.json", "reviewKey": "review-key.json" }),
    )
}

/// The invoice fixture is tiny. Stop early on unexpected size, links or binary data instead of quietly hiding work.
fn review_files(root: &Path, folder: &Path) -> Result<BTreeMap<String, String>, Failure> {
    use faber_winproc::rewind::Root;
    use std::io::Read;
    fn walk(base: &Path, held: &Root, dir: &Path, files: &mut BTreeMap<String, String>, budget: &mut u64) -> Result<(), Failure> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if [".git", ".faber"].iter().any(|name| entry.file_name() == *name) {
                continue;
            }
            let path = entry.path();
            if faber_engine::permissions::secret(&path, true) {
                return Err("invoice inputs include a protected file name".into());
            }
            let metadata = std::fs::symlink_metadata(&path)?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("unexpected reparse point in review inputs".into());
                }
            }
            if metadata.is_symlink() || *budget < 1 || metadata.len() > *budget {
                return Err("invoice review inputs contain a link or exceed the size limit".into());
            }
            *budget -= 1;
            if metadata.is_dir() {
                if path.strip_prefix(base)?.components().count() > 8 {
                    return Err("invoice review inputs exceed the depth limit".into());
                }
                walk(base, held, &path, files, budget)?;
            } else if metadata.is_file() {
                let relative = path.strip_prefix(base)?.to_str().ok_or("invoice review input path is not UTF-8")?;
                let mut text = String::new();
                held.open_file(relative)?.take(*budget + 1).read_to_string(&mut text)?;
                *budget = budget.checked_sub(text.len() as u64).ok_or("invoice review inputs exceed the size limit")?;
                files.insert(relative.replace('\\', "/"), text);
            } else {
                return Err("invoice review inputs contain a special file".into());
            }
        }
        Ok(())
    }
    let expected = root.canonicalize()?.join(folder.strip_prefix(root)?);
    let folder = folder.canonicalize()?;
    if folder != expected {
        return Err("work folder was redirected".into());
    }
    let mut files = BTreeMap::new();
    walk(&folder, &Root::open(&folder)?, &folder, &mut files, &mut (64 * 1024))?;
    Ok(files)
}

async fn prove_scoped(node: &Cli, work: &Work, side: &SideResult, root: &Path, inputs: &Path) -> Result<pure::Proof, Failure> {
    let folder = Path::new(side.folder.as_deref().ok_or("the trial did not report its project folder")?);
    if folder.canonicalize()? != work.repo.canonicalize()? {
        return Err("the trial did not work in its disposable project".into());
    }
    let codex = find_cli("codex").ok_or("no native sandbox runner is available")?;
    if !codex.prefix.is_empty()
        || !node.prefix.is_empty()
        || !Path::new(&codex.command).is_absolute()
        || !Path::new(&node.command).is_absolute()
    {
        return Err("native sandbox and Node executables are required".into());
    }
    let stage = root.join("proof");
    std::fs::create_dir(&stage)?;
    let candidate = stage.join("candidate");
    let pristine = stage.join("pristine");
    let scratch = stage.join("scratch");
    std::fs::create_dir(&scratch)?;
    for (source, copied) in [(folder, &candidate), (&work.pristine, &pristine)] {
        std::fs::create_dir(copied)?;
        for (name, contents) in review_files(root, source)? {
            let path = copied.join(name);
            std::fs::create_dir_all(path.parent().ok_or("invalid proof input path")?)?;
            std::fs::write(path, contents)?;
        }
    }
    let script = stage.join("invoice.mjs");
    let answer = stage.join("answer.txt");
    std::fs::copy(inputs.join("proofs/invoice.mjs"), &script)?;
    std::fs::write(&answer, &side.reply)?;
    let flags = ["--preserve-symlinks", "--preserve-symlinks-main"];
    let profile = format!("faber-proof-{}", uuid::Uuid::new_v4().simple());
    let filesystem = format!(
        "permissions.{profile}.filesystem={{\":root\"=\"deny\",\":minimal\"=\"read\",\":tmpdir\"=\"deny\",\":slash_tmp\"=\"deny\",\":workspace_roots\"={{\".\"=\"read\",\"scratch\"=\"write\"}},{}=\"read\"}}",
        json!(node.command)
    );
    let mut argv = args(&[
        "sandbox",
        "-c",
        &format!("permissions.{profile}.extends=\":workspace\""),
        "-c",
        &filesystem,
        "-c",
        &format!("permissions.{profile}.network={{enabled=false}}"),
        "-c",
        "shell_environment_policy.inherit=\"core\"",
        "-c",
        &format!("shell_environment_policy.set={{NODE_OPTIONS={}}}", json!(flags.join(" "))),
    ]);
    if cfg!(windows) {
        argv.extend(args(&["-c", "windows.sandbox=\"mxc\""]));
    }
    argv.extend(args(&[
        "--permission-profile",
        &profile,
        "--include-managed-config",
        "-C",
        stage.to_str().ok_or("invalid proof folder")?,
        "--",
        &node.command,
    ]));
    argv.extend(flags.map(String::from));
    for path in [&script, &candidate, &pristine, &answer, &scratch] {
        argv.push(path.to_str().ok_or("invalid proof input path")?.into());
    }
    let command = proc::marked(&codex.command, &argv, &stage);
    let run = run_child(command, &codex.command, None, PROOF_LIMIT).await?;
    if run.timed_out {
        return Err("the isolated proof timed out".into());
    }
    pure::parse_proof(&run.stdout, run.code).ok_or_else(|| {
        format!("the isolated proof did not produce a result (exit {:?}): {}", run.code, run.stderr.chars().take(400).collect::<String>())
            .into()
    })
}

fn make_review(root: &Path, request: &Request, sides: &BTreeMap<&str, serde_json::Value>) -> Result<(), Failure> {
    let mut labels = ["standard", "guided"];
    // The review's A/B order is independent of who executed first.
    if pure::a_side(&uuid::Uuid::new_v4().to_string(), 0) == Side::Faber {
        labels.reverse();
    }
    let mut entries = BTreeMap::new();
    for (letter, label) in ["A", "B"].into_iter().zip(labels) {
        let result = &sides[label]["result"];
        let side = root.join(label);
        let files = review_files(&side, &side.join("proof/candidate"))?;
        let checks: Vec<_> = result["proof"]["checks"]
            .as_array()
            .ok_or("missing proof checks")?
            .iter()
            .map(|c| json!({ "name": c["name"], "passed": c["ok"] }))
            .collect();
        entries.insert(letter, json!({ "proofPassed": result["proof"]["pass"], "checks": checks, "files": files }));
    }
    write_json(&root.join("blind-review.json"), &json!({ "request": request.prompt, "entries": entries }))?;
    write_json(&root.join("review-key.json"), &json!({ "A": labels[0], "B": labels[1] }))
}

pub(super) async fn run(options: &Options) -> Result<(), Failure> {
    if !options.only.is_empty() && options.only != [REQUEST] {
        return Err("--workflow-pair takes only the fix-invoice request".into());
    }
    if !matches!(Path::new(&options.run_id).components().collect::<Vec<_>>().as_slice(), [std::path::Component::Normal(_)]) {
        return Err("--run-id must be a single folder name".into());
    }
    let mut order = ["standard", "guided"];
    if pure::a_side(&options.run_id, 0) == Side::Faber {
        order.reverse();
    }
    let choice = Choice { vendor: "codex".into(), model: Some(MODEL.into()), effort: Some(Effort::Low) };
    println!("workflow pair: {REQUEST}, separate Faber projects on {MODEL} low, order {order:?}");
    if options.dry_run {
        load_requests(&repo_root().join("bench"), &[REQUEST.into()])?;
        return Ok(());
    }
    std::fs::create_dir_all(&options.dir)?;
    // The build coordinator may start the example from a junction. Engine data needs a physical path.
    let root = PathBuf::from(faber_engine::project::git::native_path(&options.dir.canonicalize()?)).join(&options.run_id);
    std::fs::create_dir(&root)?;
    println!("workflow receipts: {}", root.display());
    let inputs = inputs::snapshot(&repo_root().join("bench"), &root)?;
    let [request] = load_requests(&inputs, &[REQUEST.into()])?.try_into().map_err(|_| "expected one invoice request")?;
    let node = find_cli("node").ok_or("node is not on the PATH")?;
    let mut codex = find_cli("codex").ok_or("codex is not on the PATH")?;
    // Use the first-party provider and disable native delegation on both sides without editing the user's configuration.
    codex.prefix.extend(args(&[
        "-c",
        "model_provider=\"openai\"",
        "--disable",
        "multi_agent",
        "-c",
        "web_search=\"disabled\"",
        "-c",
        "mcp_servers={}",
        "--disable",
        "hooks",
        "--disable",
        "plugins",
        "--disable",
        "apps",
        "--disable",
        "browser_use",
        "--disable",
        "computer_use",
    ]));
    let clis = BTreeMap::from([(Vendor::Codex, codex)]);
    let sides_root = order.map(|side| root.join(side));
    let first = prepare(&request, &sides_root[0], &node.command, &inputs, None)?;
    let second = prepare(&request, &sides_root[1], &node.command, &inputs, Some(&first.pristine))?;
    let mut sides = BTreeMap::new();
    let mut spent = Spend::default();
    checkpoint(&root, &choice, order, &sides, None)?;
    for ((label, side_root), work) in order.into_iter().zip(&sides_root).zip([first, second]) {
        let allowance = options.cap.allowance(&spent, options.per_run_usd);
        if allowance <= 0.0 {
            let why = options.cap.reached(&spent).unwrap_or_else(|| "no allowance remains".into());
            checkpoint(&root, &choice, order, &sides, Some(&why))?;
            return Err(why.into());
        }
        let mut treatment = request.clone();
        treatment.prompt.push_str("\n\n");
        treatment.prompt.push_str(CONSTRAINTS);
        if label == "guided" {
            treatment.prompt.push('\n');
            treatment.prompt.push_str(GUIDANCE);
        }
        let mut side = run_faber(&clis, &treatment, &work, side_root, allowance, false, Some(&choice), None).await;
        let proof_error = if side.ok && side.folder.is_some() {
            match prove_scoped(&node, &work, &side, side_root, &inputs).await {
                Ok(proof) => {
                    side.proof = Some(proof);
                    None
                }
                Err(error) => Some(error.to_string()),
            }
        } else {
            None
        };
        let observation = std::fs::read(side_root.join(OBSERVATION))
            .ok()
            .and_then(|data| serde_json::from_slice::<serde_json::Value>(&data).ok())
            .unwrap_or_default();
        spent.add(&side.usage, side.cost_usd);
        let invalid = !side.ok
            || side.proof.is_none()
            || side.stopped_by_cap
            || side.timed_out
            || side.cost_usd.is_none()
            || side.usage.fresh() == 0
            || observation["tokenCountsReported"] != true
            || observation["profileMatched"] != true
            || observation["delegationObserved"] != false;
        let reason = invalid.then(|| {
            proof_error
                .or_else(|| side.error.clone())
                .unwrap_or_else(|| "a side was incomplete or violated the profile, delegation or measurement controls".into())
        });
        println!(
            "  {label}: finished {}, proof {:?}, equivalent US${:?}, {:.1}s",
            side.ok,
            side.proof.as_ref().map(|p| p.pass),
            side.cost_usd,
            side.wall_secs
        );
        sides.insert(label, json!({ "result": side, "observation": observation }));
        checkpoint(&root, &choice, order, &sides, reason.as_deref())?;
        if let Some(reason) = reason {
            return Err(reason.into());
        }
    }
    if let Err(error) = make_review(&root, &request, &sides) {
        checkpoint(&root, &choice, order, &sides, Some(&error.to_string()))?;
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use faber_engine::signals::Signals;

    fn signals(s: Signals) -> EngineEvent {
        EngineEvent::Signals(s)
    }

    #[test]
    fn the_review_rejects_hardlinks_before_reading_the_candidate() {
        let trial = tempfile::tempdir().unwrap();
        let candidate = trial.path().join("candidate");
        let other = trial.path().join("outside.txt");
        std::fs::create_dir(&candidate).unwrap();
        std::fs::write(&other, "outside").unwrap();
        std::fs::write(candidate.join("edit.txt"), "own").unwrap();
        std::fs::create_dir(candidate.join(".faber")).unwrap();
        std::fs::write(candidate.join(".faber/diagnostic.txt"), "run noise").unwrap();
        assert_eq!(review_files(trial.path(), &candidate).unwrap(), BTreeMap::from([("edit.txt".into(), "own".into())]));
        std::fs::hard_link(&other, candidate.join("other.txt")).unwrap();
        assert!(review_files(trial.path(), &candidate).is_err());
    }

    #[test]
    fn a_subscription_without_explicit_no_credits_never_starts_a_side() {
        let plan = signals(Signals { billing: Some(Billing::Subscription), ..Default::default() });
        assert!(!account_is_safe(std::slice::from_ref(&plan)));
        let no_credits = signals(Signals { paid_fallback: Some(false), ..Default::default() });
        assert!(!account_is_safe(std::slice::from_ref(&no_credits)));
        assert!(account_is_safe(&[plan.clone(), no_credits.clone()]));
        assert!(!account_is_safe(&[
            plan.clone(),
            no_credits.clone(),
            EngineEvent::Quota { window: "week".into(), used: 1.0, resets_at: 100.0 }
        ]));
        for unsafe_signal in [
            Signals { billing: Some(Billing::UsageBilled), ..Default::default() },
            Signals { paid_fallback: Some(true), ..Default::default() },
            Signals { overage_active: Some(true), ..Default::default() },
            Signals { credit_balance: Some(1.0), ..Default::default() },
            Signals { reach: Some(Reach::Out(None)), ..Default::default() },
        ] {
            assert!(!account_is_safe(&[plan.clone(), no_credits.clone(), signals(unsafe_signal)]));
        }
    }
}
