//! The order of checks and installs, independent of the shell.

use crate::{Phase, gate::Who};
use std::time::Duration;
#[cfg(any(windows, test))]
use std::{io, path::Path};
use tokio::time::timeout;

pub(crate) fn fetch_succeeded(found: bool, downloaded: bool) -> bool {
    !found || downloaded
}

/// Everything the start and the idle install need from the world, so the order of the steps is tested without a network, a
/// window or an installer.
#[allow(async_fn_in_trait)]
pub trait Release {
    /// Looks for a newer release: its version when there is one.
    async fn check(&self) -> Result<Option<String>, String>;
    /// This version was already tried less than a day ago: it is not installed unattended again yet.
    fn tried(&self, version: &str) -> bool;
    /// Downloads the release found and keeps it ready.
    async fn download(&self) -> Result<(), String>;
    /// What the screen shows of the install at open (`Phase::None` when it ends).
    fn stage(&self, phase: Phase);
    /// Decides the install, all at once (`admit`): nothing is running (an answer, a CLI
    /// install), no unsent draft is on the screen, the tried marker is on the disk, and from here on nothing new may start.
    /// False when anything stops it.
    fn reserve(&self, who: Who) -> bool;
    fn shut_down(&self);
    /// Starts the installer; only a failure comes back.
    fn install(&self) -> Result<(), String>;
    fn relaunch(&self);
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Start {
    NothingNew,
    /// This version was tried less than a day ago: nothing is downloaded or installed unattended.
    Skipped,
    /// No answer from the server in time, or the download failed or took too long: tried again soon.
    Unreachable,
    Installed,
    /// Work began, or the person started typing, during the download: the update waits for the idle install.
    Waits,
    /// The installer did not start; reopening Faber was requested.
    Failed,
}

impl Start {
    pub fn succeeded(self) -> bool {
        matches!(self, Self::NothingNew | Self::Skipped | Self::Installed | Self::Waits)
    }
}

/// Faber just opened: look for a release, download it, and install it before any work begins. If work started meanwhile,
/// or the person has typed anything (a draft lives only on the screen), nothing is ended: the update waits for an idle
/// moment, or for the person's "Update ready, restart".
pub async fn update_on_start(release: &impl Release, check_wait: Duration, download_wait: Duration) -> Start {
    release.stage(Phase::Checking);
    let start = start_steps(release, check_wait, download_wait).await;
    // An install ends the process; anything else gives the screen back to the person.
    if start != Start::Installed {
        release.stage(Phase::None);
    }
    start
}

async fn start_steps(release: &impl Release, check_wait: Duration, download_wait: Duration) -> Start {
    let version = match timeout(check_wait, release.check()).await {
        Ok(Ok(Some(version))) => version,
        Ok(Ok(None)) => return Start::NothingNew,
        Ok(Err(_)) | Err(_) => return Start::Unreachable,
    };
    if release.tried(&version) {
        return Start::Skipped;
    }
    release.stage(Phase::Downloading);
    if !matches!(timeout(download_wait, release.download()).await, Ok(Ok(()))) {
        return Start::Unreachable;
    }
    release.stage(Phase::Installing);
    match idle_install(|| true, || release.reserve(Who::Start), || release.shut_down(), || release.install(), || release.relaunch()) {
        Idle::Installed => Start::Installed,
        // How Faber comes back is decided when it installs (`handoff`), so the kept update waits as it is.
        Idle::Waited => Start::Waits,
        Idle::Failed => Start::Failed,
    }
}

/// Looks for a newer release and downloads it. False when the check or the download failed.
pub async fn fetch(release: &impl Release, check_wait: Duration, download_wait: Duration) -> bool {
    match timeout(check_wait, release.check()).await {
        Err(_) | Ok(Err(_)) => false,
        Ok(Ok(None)) => fetch_succeeded(false, false),
        // A version tried a day ago is left alone: it would only be refused again.
        Ok(Ok(Some(version))) if release.tried(&version) => true,
        Ok(Ok(Some(_))) => fetch_succeeded(true, matches!(timeout(download_wait, release.download()).await, Ok(Ok(())))),
    }
}

#[derive(Debug, PartialEq)]
pub enum Idle {
    /// Not idle, or something is running: nothing was touched.
    Waited,
    Installed,
    /// The installer did not start; reopening Faber was requested.
    Failed,
}

/// The install in order: the cheap look, the one decision that reserves it against everything that could start, clean up,
/// install, and relaunch if the installer did not start.
pub fn idle_install(
    idle: impl Fn() -> bool,
    reserve: impl FnOnce() -> bool,
    shut_down: impl FnOnce(),
    install: impl FnOnce() -> Result<(), String>,
    relaunch: impl FnOnce(),
) -> Idle {
    if !idle() || !reserve() {
        return Idle::Waited;
    }
    shut_down();
    match install() {
        Ok(()) => Idle::Installed,
        Err(_) => {
            relaunch();
            Idle::Failed
        }
    }
}

#[cfg(any(windows, test))]
pub(crate) trait Clock {
    fn now(&self) -> Duration;
    fn sleep(&self, duration: Duration);
}

/// Returns the last observation, including when the deadline passes.
#[cfg(any(windows, test))]
pub(crate) fn poll<T>(clock: &impl Clock, limit: Duration, mut query: impl FnMut() -> T, done: impl Fn(&T) -> bool) -> T {
    let began = clock.now();
    loop {
        let result = query();
        let left = limit.saturating_sub(clock.now() - began);
        if done(&result) || left.is_zero() {
            return result;
        }
        clock.sleep(left.min(Duration::from_millis(250)));
    }
}

#[cfg(any(windows, test))]
const REOPEN_WAIT: Duration = Duration::from_secs(60);
#[cfg(any(windows, test))]
const REOPEN_CLOSE_WAIT: Duration = Duration::from_secs(5);

#[cfg(any(windows, test))]
pub(crate) fn reopen<P>(
    data: &Path,
    clock: &impl Clock,
    free: impl Fn() -> bool,
    mut launch: impl FnMut() -> io::Result<P>,
    mut alive: impl FnMut(&mut P) -> io::Result<bool>,
    mut read_startup: impl FnMut() -> io::Result<String>,
) -> bool {
    let began = clock.now();
    let mut attempt = || {
        if !poll(clock, REOPEN_CLOSE_WAIT, &free, |free| *free) {
            return Err("could not confirm the previous Faber closed".to_string());
        }
        let before = read_startup().ok();
        let mut child = launch().map_err(|error| format!("could not start Faber: {error}"))?;
        let (running, host_started) = poll(
            clock,
            REOPEN_WAIT,
            || {
                let running = alive(&mut child);
                let host_started = !matches!(running, Ok(false))
                    && before.as_deref().is_some_and(|prior| read_startup().is_ok_and(|log| new_host_line(prior, &log)));
                (running, host_started)
            },
            |(running, host_started)| *host_started || matches!(running, Ok(false)),
        );
        match running {
            // If the log is unavailable, a child still running at the deadline survived single-instance startup.
            Ok(true) => Ok(()),
            Ok(false) => Err("Faber exited before startup was confirmed".into()),
            Err(_) if host_started => Ok(()),
            Err(error) => Err(format!("could not check whether Faber reopened: {error}")),
        }
    };
    match attempt().or_else(|error| {
        crate::log(data, "reopen-retry", &error);
        attempt()
    }) {
        Ok(()) => {
            crate::log(data, "reopened", &format!("in {} ms", (clock.now() - began).as_millis()));
            true
        }
        Err(error) => {
            crate::log(data, "reopen-failed", &error);
            false
        }
    }
}

/// Compare complete records across log trimming and same-second starts with identical text.
#[cfg(any(windows, test))]
fn new_host_line(before: &str, after: &str) -> bool {
    if before == after {
        return false;
    }
    let Some(latest) = host_lines(after).last() else { return false };
    host_lines(after).filter(|line| *line == latest).count() > host_lines(before).filter(|line| *line == latest).count()
}

#[cfg(any(windows, test))]
fn host_lines(log: &str) -> impl Iterator<Item = &str> {
    log.split_inclusive('\n').filter(|line| {
        line.ends_with('\n')
            && line.split_once(" host ").is_some_and(|(secs, _)| !secs.is_empty() && secs.bytes().all(|b| b.is_ascii_digit()))
    })
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct FakeClock(std::cell::Cell<Duration>);

#[cfg(test)]
impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.0.get()
    }

    fn sleep(&self, duration: Duration) {
        self.0.set(self.now() + duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// A release with nothing real behind it: it records what the start did, in order.
    struct Fake {
        found: Result<Option<String>, String>,
        /// The check never answers (a network that is down without saying so).
        hangs: bool,
        /// The download never ends.
        stalls: bool,
        download: Result<(), String>,
        /// A turn begins while the download runs.
        work_during_download: bool,
        /// The person types while the download runs.
        typing_during_download: bool,
        /// This version was tried less than a day ago.
        tried: bool,
        install: Result<(), String>,
        busy: Cell<bool>,
        editing: Cell<bool>,
        seen: RefCell<Vec<&'static str>>,
    }

    impl Fake {
        fn found() -> Self {
            Self {
                found: Ok(Some("1.2.3".into())),
                hangs: false,
                stalls: false,
                download: Ok(()),
                work_during_download: false,
                typing_during_download: false,
                tried: false,
                install: Ok(()),
                busy: Cell::new(false),
                editing: Cell::new(false),
                seen: RefCell::new(vec![]),
            }
        }
        fn saw(&self) -> Vec<&'static str> {
            self.seen.borrow().clone()
        }
    }

    impl Release for Fake {
        async fn check(&self) -> Result<Option<String>, String> {
            self.seen.borrow_mut().push("check");
            if self.hangs {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
            self.found.clone()
        }
        fn tried(&self, _: &str) -> bool {
            self.tried
        }
        async fn download(&self) -> Result<(), String> {
            self.seen.borrow_mut().push("download");
            if self.stalls {
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
            self.busy.set(self.work_during_download);
            self.editing.set(self.typing_during_download);
            self.download.clone()
        }
        fn stage(&self, phase: Phase) {
            self.seen.borrow_mut().push(match phase {
                Phase::None => "none",
                Phase::Checking => "checking",
                Phase::Downloading => "downloading",
                Phase::Installing => "installing",
                Phase::Ready => "ready",
            });
        }
        fn reserve(&self, _: Who) -> bool {
            self.seen.borrow_mut().push("reserve");
            !self.busy.get() && !self.editing.get()
        }
        fn shut_down(&self) {
            self.seen.borrow_mut().push("shut down");
        }
        fn install(&self) -> Result<(), String> {
            self.seen.borrow_mut().push("install");
            self.install.clone()
        }
        fn relaunch(&self) {
            self.seen.borrow_mut().push("relaunch");
        }
    }

    const WAIT: Duration = Duration::from_millis(50);

    async fn start(release: &Fake) -> Start {
        update_on_start(release, WAIT, WAIT).await
    }

    fn update_log(data: &Path) -> String {
        std::fs::read_to_string(data.join("update.log")).unwrap()
    }

    #[test]
    fn an_app_that_exits_at_once_is_started_again_after_it_releases_its_files() {
        let data = tempfile::tempdir().unwrap();
        let clock = FakeClock::default();
        let launches = Cell::new(0);
        let released_after = Duration::from_millis(750);
        assert!(reopen(
            data.path(),
            &clock,
            || launches.get() == 0 || clock.now() >= released_after,
            || {
                let attempt = launches.get() + 1;
                launches.set(attempt);
                Ok(attempt)
            },
            |attempt| Ok(*attempt == 2),
            || {
                let old = "1700000000 host engine=12ms\n";
                // The second start writes the same host line in the same second.
                Ok(if launches.get() == 2 { old.repeat(2) } else { old.into() })
            },
        ));
        assert_eq!(launches.get(), 2);
        let log = update_log(data.path());
        assert!(log.contains("reopen-retry Faber exited before startup was confirmed"));
        assert!(log.contains("reopened in 750 ms"));
    }

    #[test]
    fn without_startup_proof_the_helper_logs_failure_after_one_retry() {
        let data = tempfile::tempdir().unwrap();
        let clock = FakeClock::default();
        let mut launches = 0;
        assert!(!reopen(
            data.path(),
            &clock,
            || true,
            || {
                launches += 1;
                Ok(())
            },
            |_| Err(io::Error::other("query denied")),
            || Ok("1700000000 host engine=12ms\n".into()),
        ));
        assert_eq!(launches, 2);
        assert_eq!(clock.now(), 2 * REOPEN_WAIT);
        let log = update_log(data.path());
        assert!(log.contains("reopen-failed could not check whether Faber reopened: query denied"));
        assert!(!log.contains("reopened in"));
    }

    #[test]
    fn a_new_host_line_logs_reopened_without_waiting_for_the_deadline() {
        let data = tempfile::tempdir().unwrap();
        let clock = FakeClock::default();
        let started_at = Duration::from_millis(500);
        let mut launches = 0;
        assert!(reopen(
            data.path(),
            &clock,
            || true,
            || {
                launches += 1;
                Ok(())
            },
            |_| Ok(true),
            || {
                let log = if clock.now() >= started_at {
                    "1700000001 host engine=14ms\n"
                } else {
                    "1699999980 shown saved=2ms\n1700000000 host engine=12ms\n"
                };
                Ok(log.into())
            },
        ));
        assert_eq!(launches, 1);
        assert!(update_log(data.path()).contains("reopened in 500 ms"));
    }

    #[test]
    fn an_old_or_partial_host_line_does_not_prove_startup() {
        let data = tempfile::tempdir().unwrap();
        let clock = FakeClock::default();
        assert!(reopen(
            data.path(),
            &clock,
            || true,
            || Ok(()),
            |_| Ok(true),
            || {
                let old = "1699999999 host engine=27ms\n";
                let log = if clock.now().is_zero() { format!("{old}1700000000 host engine=12ms\n") } else { old.into() };
                Ok(log + "1700000001 host engine=14ms")
            },
        ));
        assert_eq!(clock.now(), REOPEN_WAIT, "only the process surviving to the deadline proves startup");
        assert!(update_log(data.path()).contains("reopened in 60000 ms"));
    }

    #[tokio::test]
    async fn a_release_found_at_start_is_downloaded_and_installed_before_any_work() {
        let release = Fake::found();
        assert_eq!(start(&release).await, Start::Installed);
        assert_eq!(release.saw(), ["checking", "check", "downloading", "download", "installing", "reserve", "shut down", "install"]);
    }

    #[tokio::test]
    async fn nothing_new_at_start_touches_nothing() {
        let release = Fake { found: Ok(None), ..Fake::found() };
        assert_eq!(start(&release).await, Start::NothingNew);
        assert_eq!(release.saw(), ["checking", "check", "none"]);
    }

    #[tokio::test]
    async fn an_offline_start_gives_up_in_time_and_tries_again_soon() {
        let refused = Fake { found: Err("offline".into()), ..Fake::found() };
        assert_eq!(start(&refused).await, Start::Unreachable);
        assert_eq!(refused.saw(), ["checking", "check", "none"]);
        let silent = Fake { hangs: true, ..Fake::found() };
        let began = std::time::Instant::now();
        assert_eq!(start(&silent).await, Start::Unreachable);
        assert!(began.elapsed() < Duration::from_secs(5), "the start does not wait for a dead network");
        assert_eq!(silent.saw(), ["checking", "check", "none"], "the opening screen goes away with the check");
        assert!(!Start::Unreachable.succeeded() && !Start::Failed.succeeded());
        assert!(Start::NothingNew.succeeded() && Start::Installed.succeeded() && Start::Waits.succeeded() && Start::Skipped.succeeded());
    }

    #[tokio::test]
    async fn a_turn_started_during_the_download_is_never_ended() {
        let release = Fake { work_during_download: true, ..Fake::found() };
        assert_eq!(start(&release).await, Start::Waits);
        assert_eq!(
            release.saw(),
            ["checking", "check", "downloading", "download", "installing", "reserve", "none"],
            "no cleanup, no installer, no relaunch; the kept update waits as it is"
        );
    }

    #[tokio::test]
    async fn typing_during_the_download_keeps_the_draft_and_defers_the_install() {
        let release = Fake { typing_during_download: true, ..Fake::found() };
        assert_eq!(start(&release).await, Start::Waits);
        assert_eq!(
            release.saw(),
            ["checking", "check", "downloading", "download", "installing", "reserve", "none"],
            "the decision reads the draft inside the lock and gives way"
        );
    }

    #[tokio::test]
    async fn a_download_that_failed_installs_nothing() {
        let release = Fake { download: Err("cut".into()), ..Fake::found() };
        assert_eq!(start(&release).await, Start::Unreachable);
        assert_eq!(release.saw(), ["checking", "check", "downloading", "download", "none"]);
    }

    #[tokio::test]
    async fn a_download_that_never_ends_is_given_up_and_the_line_goes_away() {
        let release = Fake { stalls: true, ..Fake::found() };
        let began = std::time::Instant::now();
        assert_eq!(start(&release).await, Start::Unreachable);
        assert!(began.elapsed() < Duration::from_secs(5));
        assert_eq!(release.saw(), ["checking", "check", "downloading", "download", "none"]);
        // A download in the background is bounded too, so the six-hour loop is never held for good.
        assert!(!fetch(&Fake { stalls: true, ..Fake::found() }, WAIT, WAIT).await);
        assert!(!fetch(&Fake { hangs: true, ..Fake::found() }, WAIT, WAIT).await);
    }

    #[tokio::test]
    async fn an_installer_that_does_not_start_leaves_a_relaunched_faber() {
        let release = Fake { install: Err("blocked".into()), ..Fake::found() };
        assert_eq!(start(&release).await, Start::Failed);
        // The real relaunch ends the process, so the screen is never told "none" after it.
        assert!(release.saw().ends_with(&["install", "relaunch", "none"]));
    }

    #[tokio::test]
    async fn a_version_tried_a_day_ago_is_not_downloaded_or_installed_again_at_start() {
        let release = Fake { tried: true, ..Fake::found() };
        assert_eq!(start(&release).await, Start::Skipped);
        assert_eq!(
            release.saw(),
            ["checking", "check", "none"],
            "the relaunch after a failed install does not start the same install again"
        );
        let later = Fake { tried: true, ..Fake::found() };
        assert!(fetch(&later, WAIT, WAIT).await);
        assert_eq!(later.saw(), ["check"], "nor does the periodic check download it");
    }
}
