//! Account admission for the manual benchmark, separate from Keep going.
use faber_engine::{
    account::{UsageRead, claude_usage},
    budget::vendor_wide,
    cadence,
    cli::Cli,
    events::EngineEvent,
    ledger::{LAG_CLAUDE, LAG_CODEX},
    model::Vendor,
    proc,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use super::pure::quiet as policy;

#[derive(Default)]
struct Account {
    quiet: policy::Quiet,
    next: f64,
    quiet_at: Option<f64>,
}

pub struct Accounts {
    root: PathBuf,
    accounts: BTreeMap<Vendor, Account>,
}

fn now() -> f64 {
    chrono::Utc::now().timestamp_millis() as f64 / 1000.0
}

impl Accounts {
    pub fn new(root: &Path) -> Self {
        Self { root: root.into(), accounts: BTreeMap::new() }
    }

    pub fn worked(&mut self, vendors: &[Vendor]) {
        let now = now();
        for vendor in vendors {
            let account = self.accounts.entry(*vendor).or_default();
            let lag = match vendor {
                Vendor::Claude => LAG_CLAUDE,
                Vendor::Codex => LAG_CODEX,
                Vendor::Gemini => continue,
            };
            account.quiet.own_work_ended(now, lag);
            account.next = now + lag;
            account.quiet_at = None;
        }
    }

    pub async fn wait(&mut self, clis: &BTreeMap<Vendor, Cli>, vendors: &[Vendor], spent: &mut super::pure::Spend) -> Result<(), String> {
        println!("  checking fresh quiet intervals; create {} to cancel", self.root.join("CANCEL").display());
        loop {
            if self.root.join("CANCEL").exists() {
                return Err("benchmark cancelled; completed results were saved".into());
            }
            for vendor in vendors {
                if self.root.join("CANCEL").exists() {
                    return Err("benchmark cancelled; completed results were saved".into());
                }
                let cli = clis.get(vendor).ok_or("quota CLI is not installed")?;
                let account = self.accounts.entry(*vendor).or_default();
                let time = now();
                if time < account.next {
                    continue;
                }
                let spawn = status_spawner(*vendor, cli, proc::real_spawner());
                let events = match vendor {
                    Vendor::Gemini => continue,
                    Vendor::Claude => match claude_usage(
                        cli,
                        &spawn,
                        &self.root,
                        Duration::from_secs(45),
                        time,
                        chrono::Local::now().offset().local_minus_utc() as i64,
                    )
                    .await
                    {
                        UsageRead::Windows(windows) => windows
                            .into_iter()
                            .map(|(window, used, resets_at)| EngineEvent::Quota { window: window.into(), used, resets_at })
                            .collect(),
                        UsageRead::Failed => Vec::new(),
                        UsageRead::Spent => {
                            spent.unknown_cost = true;
                            return Err("quota status unexpectedly used a model; spending is unknown, no more work admitted".into());
                        }
                    },
                    Vendor::Codex => {
                        faber_engine::codex::read_codex_limits(&cli.command, &self.root, cfg!(windows), spawn, Duration::from_secs(20))
                            .await
                    }
                };
                let time = now();
                let gap = cadence::gap(*vendor);
                account.next = time + gap;
                let samples = samples(events, time);
                account.quiet_at = account.quiet.observe(time, gap, samples).then_some(time);
                if account.quiet_at.is_none() {
                    println!("  {}: paused until fresh readings show no usage change", vendor.name());
                }
            }
            if vendors.iter().all(|v| self.accounts[v].quiet_at.is_some_and(|at| (0.0..=cadence::gap(*v)).contains(&(now() - at)))) {
                if self.root.join("CANCEL").exists() {
                    return Err("benchmark cancelled; completed results were saved".into());
                }
                return Ok(());
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

fn status_spawner(vendor: Vendor, cli: &Cli, real: proc::Spawner) -> proc::Spawner {
    // claude_usage already adds its CLI prefix; the Codex server reader takes a bare binary.
    if vendor == Vendor::Claude {
        return real;
    }
    let prefix = cli.prefix.clone();
    Arc::new(move |bin, args, cwd| real(bin, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd))
}

fn samples(events: Vec<EngineEvent>, time: f64) -> BTreeMap<String, policy::Sample> {
    let mut samples = BTreeMap::new();
    for event in events {
        if let EngineEvent::Quota { window, used, resets_at } = event
            && vendor_wide(&window)
        {
            // Keep invalid evidence for the policy to reject; filtering it can falsely establish quiet.
            if samples.insert(window, policy::Sample { at: time, used, reset: resets_at }).is_some() {
                return BTreeMap::new();
            }
        }
    }
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancellation_before_admission_never_reads_an_account_or_changes_spending() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CANCEL"), "").unwrap();
        let mut accounts = Accounts::new(dir.path());
        accounts.worked(&[Vendor::Codex]);
        let mut spent = super::super::pure::Spend { usd: 0.75, ..Default::default() };
        let before = spent;
        // No CLI is available: an attempt to read one fails this check instead of making a real call.
        let error = accounts.wait(&BTreeMap::new(), &[Vendor::Codex], &mut spent).await.unwrap_err();
        assert!(error.contains("cancelled"), "{error}");
        assert_eq!(spent, before);
    }

    #[test]
    fn an_invalid_window_cannot_disappear_behind_a_valid_one() {
        let mut quiet = policy::Quiet::default();
        let events = || {
            vec![
                EngineEvent::Quota { window: "300_minutes".into(), used: 0.2, resets_at: 5000. },
                EngineEvent::Quota { window: "10080_minutes".into(), used: 0.1, resets_at: 50. },
            ]
        };
        assert!(!quiet.observe(100., 60., samples(events(), 100.)));
        assert!(!quiet.observe(160., 60., samples(events(), 160.)));
    }

    #[tokio::test]
    async fn npm_status_reads_prefix_the_script_exactly_once() {
        for vendor in Vendor::ALL {
            let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
            let captured = seen.clone();
            let fake: proc::Spawner = Arc::new(move |_, args, _| {
                *captured.lock().unwrap() = args.to_vec();
                Err(std::io::Error::other("no process in this check"))
            });
            let cli = Cli { command: "node".into(), prefix: vec!["tool.js".into()] };
            let spawn = status_spawner(vendor, &cli, fake);
            if vendor == Vendor::Claude {
                let _ = claude_usage(&cli, &spawn, Path::new("."), Duration::from_secs(1), 100., 0).await;
            } else {
                let _ = faber_engine::codex::read_codex_limits(&cli.command, Path::new("."), cfg!(windows), spawn, Duration::from_secs(1))
                    .await;
            }
            assert_eq!(seen.lock().unwrap().iter().filter(|s| s.as_str() == "tool.js").count(), 1, "{vendor:?}");
        }
    }
}
