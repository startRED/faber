//! In-memory controls for the benchmark's disposable engines.
use super::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchPart {
    Persona,
    Guide,
    StartCard,
    Checks,
    Review,
    Router,
    Title,
    AskJudge,
    FollowUps,
    Delegation,
    CreativeJudge,
}

impl BenchPart {
    pub const ALL: [Self; 11] = [
        Self::Persona,
        Self::Guide,
        Self::StartCard,
        Self::Checks,
        Self::Review,
        Self::Router,
        Self::Title,
        Self::AskJudge,
        Self::FollowUps,
        Self::Delegation,
        Self::CreativeJudge,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Persona => "persona",
            Self::Guide => "guide",
            Self::StartCard => "start-card",
            Self::Checks => "checks",
            Self::Review => "review",
            Self::Router => "router",
            Self::Title => "title",
            Self::AskJudge => "ask-judge",
            Self::FollowUps => "follow-ups",
            Self::Delegation => "delegation",
            Self::CreativeJudge => "creative-judge",
        }
    }
}

#[derive(Default)]
pub(super) struct BenchSwitches {
    disabled: Option<BenchPart>,
    calls: Option<u64>,
    primary_started: bool,
}

impl BenchSwitches {
    pub(super) fn off(&self, part: BenchPart) -> bool {
        self.disabled == Some(part)
    }

    pub(super) fn launch_tuning(&self, tuning: crate::model::Tuning) -> crate::model::Tuning {
        crate::model::Tuning {
            bench_persona_off: self.off(BenchPart::Persona),
            bench_review_off: self.off(BenchPart::Review),
            bench_creative_off: self.off(BenchPart::CreativeJudge),
            bench_checks_off: self.off(BenchPart::Checks),
            ..tuning
        }
    }

    pub(super) fn call_started(&mut self) {
        if let Some(calls) = &mut self.calls {
            *calls += 1;
        }
    }

    pub(super) fn primary_started(&mut self) {
        if self.calls.is_some() {
            self.primary_started = true;
        }
    }
}

impl Engine {
    /// Set once on a fresh, private benchmark engine before sending a request.
    #[doc(hidden)]
    pub fn set_bench_part(&self, disabled: Option<BenchPart>) {
        self.with(|core| {
            core.bench = BenchSwitches { disabled, calls: Some(0), primary_started: false };
        });
    }

    #[doc(hidden)]
    pub fn bench_pending_title(&self, id: &str) -> bool {
        self.with(|core| core.titles.contains_key(id))
    }

    /// Engine-started invocations beyond the first foreground turn.
    #[doc(hidden)]
    pub fn bench_extra_calls(&self) -> u64 {
        self.with(|core| core.bench.calls.unwrap_or_default().saturating_sub(u64::from(core.bench.primary_started)))
    }
}
