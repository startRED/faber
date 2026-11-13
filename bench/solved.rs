//! Cost per solved task (harness study C, idea 6): a model or a change is judged by what it costs per request whose
//! executable proof passed, not by price per token, next to how often a run hit the time limit, how often the work already
//! passed its proof when it did (a stop that threw a good result away), and whether the model the CLI ran is the one asked
//! for (idea 4a). Pure, like the rest of `pure.rs`; the runner appends it to `tables.md`.

use super::{Row, SideResult};

/// Totals of one side over a run.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Solved {
    pub runs: usize,
    /// Runs with an executable proof.
    pub proved: usize,
    pub solved: usize,
    pub timed_out: usize,
    /// Timed out, yet the folder already passed its proof.
    pub passed_at_timeout: usize,
    /// Dollars of every run, solved or not; None when any run's cost is unknown.
    pub usd: Option<f64>,
    pub fresh_tokens: u64,
    /// Runs whose CLI ran another model than the one asked for, as "asked → ran".
    pub swapped: Vec<String>,
}

impl Solved {
    /// What a solved request cost: every run's dollars over the solved ones. None with nothing solved or a cost unknown.
    pub fn usd_per_solved(&self) -> Option<f64> {
        self.usd.filter(|_| self.solved > 0).map(|usd| usd / self.solved as f64)
    }

    pub fn tokens_per_solved(&self) -> Option<u64> {
        (self.solved > 0).then(|| self.fresh_tokens / self.solved as u64)
    }
}

/// One side's totals over the rows (`pick` chooses the side).
pub fn tally(rows: &[Row], pick: fn(&Row) -> &Option<SideResult>) -> Solved {
    let mut s = Solved { usd: Some(0.0), ..Solved::default() };
    for side in rows.iter().filter_map(|r| pick(r).as_ref()) {
        s.runs += 1;
        let pass = side.proof.as_ref().map(|p| p.pass);
        s.proved += usize::from(pass.is_some());
        s.solved += usize::from(pass == Some(true) && !side.timed_out);
        s.timed_out += usize::from(side.timed_out);
        s.passed_at_timeout += usize::from(side.timed_out && pass == Some(true));
        s.usd = s.usd.zip(side.cost_usd).map(|(total, cost)| total + cost);
        s.fresh_tokens += side.usage.fresh();
        if let Some(asked) = side.requested_model.as_deref() {
            for ran in side.models.iter().filter(|ran| !same_model(asked, ran)) {
                s.swapped.push(format!("{asked} → {ran}"));
            }
        }
    }
    s
}

/// The model a CLI names against the one asked: Claude Code may add a context suffix ("[1m]") and Faber's own rows read
/// "model@effort (route)"; Auto asked for no model by name.
fn same_model(asked: &str, ran: &str) -> bool {
    let bare = |m: &str| m.split(['@', '[', ' ']).next().unwrap_or(m).trim().to_ascii_lowercase();
    asked == "auto" || bare(asked) == bare(ran)
}

/// Which side of a row a column reads.
type ReadSide = fn(&Row) -> &Option<SideResult>;

/// The section `tables.md` adds after the main table.
pub fn render(rows: &[Row]) -> String {
    let mut out = String::from(
        "\n## Cost per solved request\n\nSolved: the executable proof passed and the run ended inside its time limit. Requests \
         with no proof count as runs, never as solved.\n\n\
         | Side | Runs | With a proof | Solved | Timed out | Passed at the timeout | US$ in all | US$ per solved | First-time tokens per solved |\n\
         | :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
    );
    let money = |usd: Option<f64>| usd.map_or("unknown".into(), |u| format!("{u:.3}"));
    let sides: [(&str, ReadSide); 2] = [("Faber", |r| &r.faber), ("Plain", |r| &r.plain)];
    let mut swapped = vec![];
    for (name, pick) in sides {
        let s = tally(rows, pick);
        out.push_str(&format!(
            "| {name} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            s.runs,
            s.proved,
            s.solved,
            s.timed_out,
            s.passed_at_timeout,
            money(s.usd),
            s.usd_per_solved().map_or("-".into(), |u| format!("{u:.3}")),
            s.tokens_per_solved().map_or("-".into(), |t| t.to_string()),
        ));
        swapped.extend(s.swapped.into_iter().map(|w| format!("{name}: {w}")));
    }
    if swapped.is_empty() {
        out.push_str("\nEvery CLI ran the model it was asked for.\n");
    } else {
        out.push_str(&format!("\nA CLI ran another model than the one asked for: {}.\n", swapped.join("; ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;

    fn side(pass: Option<bool>, usd: Option<f64>, timed_out: bool) -> SideResult {
        SideResult {
            side: Side::Plain,
            ok: pass == Some(true),
            error: None,
            secs: 1.0,
            wall_secs: 1.0,
            usage: Usage { input: 100, output: 10, cache_read: 0, cache_write: 0 },
            usage_incomplete: false,
            cost_usd: usd,
            models: vec!["claude-haiku-5-5".into()],
            requested_model: Some("claude-haiku-5-5".into()),
            authors: Authors::default(),
            reply: String::new(),
            folder: None,
            proof: pass.map(|pass| Proof { pass, checks: vec![] }),
            stopped_by_cap: false,
            timed_out,
        }
    }

    fn row(plain: SideResult) -> Row {
        Row {
            request: "r".into(),
            title: "t".into(),
            kind: Kind::Project,
            a: Side::Faber,
            verdict: None,
            winner: None,
            faber: None,
            plain: Some(plain),
            judge_cost_usd: None,
            judge_attempted: false,
            judge: None,
            judge_usage: Usage::default(),
            skipped: None,
        }
    }

    #[test]
    fn a_failed_run_still_costs_and_the_price_is_per_solved_request() {
        let rows = [row(side(Some(true), Some(1.0), false)), row(side(Some(false), Some(1.0), false))];
        let s = tally(&rows, |r| &r.plain);
        assert_eq!((s.runs, s.proved, s.solved), (2, 2, 1));
        assert_eq!(s.usd_per_solved(), Some(2.0), "two dollars spent, one request solved");
        assert_eq!(s.tokens_per_solved(), Some(220));
        assert!(render(&rows).contains("| Plain | 2 | 2 | 1 | 0 | 0 | 2.000 | 2.000 | 220 |"), "{}", render(&rows));
    }

    #[test]
    fn a_timeout_is_not_solved_but_a_proof_it_already_passed_is_counted() {
        let rows = [row(side(Some(true), Some(0.5), true)), row(side(None, None, false))];
        let s = tally(&rows, |r| &r.plain);
        assert_eq!((s.solved, s.timed_out, s.passed_at_timeout), (0, 1, 1));
        assert_eq!(s.usd, None, "one cost unknown makes the total unknown, never zero");
        assert_eq!(s.usd_per_solved(), None);
    }

    #[test]
    fn a_model_the_cli_swapped_is_named() {
        let mut swapped = side(Some(true), Some(1.0), false);
        swapped.models = vec!["claude-sonnet-5-5".into(), "claude-haiku-5-5[1m]".into()];
        let rows = [row(swapped)];
        assert_eq!(tally(&rows, |r| &r.plain).swapped, vec!["claude-haiku-5-5 → claude-sonnet-5-5".to_string()]);
        assert!(render(&rows).contains("A CLI ran another model than the one asked for"));
        assert!(same_model("auto", "gpt-5.5@high (reason.route.code.hard)"));
    }
}
