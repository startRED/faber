//! The UX sentinel's judgement: numbers in, findings out. The probe (`probe.js`) and the scenarios
//! (`screen/sentinel.rs`) only measure; every rule lives here, so each rule and its planted twin is a unit test that
//! needs no app. See docs/superpowers/specs/2026-10-12-ux-sentinel-design.md.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;

/// How bad a finding is (the spec's S1, S2 and S3). Only `Critical` and `High` stop a merge, a stable update or a release
/// (decision R104); `Medium` becomes a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Critical,
    High,
    Medium,
}

/// Default severity of each detector, from the spec's table (section 10.2).
pub fn severity_of(kind: &str) -> Severity {
    match kind {
        "T1" | "M6" | "CLOCK" => Severity::Critical,
        "M1" | "I1" | "I2" | "I3" | "I4" | "I5" | "L4" => Severity::High,
        _ => Severity::Medium,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub kind: &'static str,
    pub screen: String,
    pub surface: String,
    pub element: String,
    /// Where it showed: width, look, language, size and motion level.
    pub config: String,
    /// The measured value and the limit, in words.
    pub metric: String,
}

impl Finding {
    pub fn new(kind: &'static str, screen: &str, surface: &str, element: &str, config: &str, metric: impl Into<String>) -> Finding {
        Finding { kind, screen: screen.into(), surface: surface.into(), element: element.into(), config: config.into(), metric: metric.into() }
    }

    pub fn severity(&self) -> Severity {
        severity_of(self.kind)
    }

    /// The same bug in 30 configurations is one finding, so the configuration is left out.
    pub fn fingerprint(&self) -> String {
        let text = format!("{}|{}|{}|{}", self.kind, self.screen, self.surface, self.element);
        Sha256::digest(text.as_bytes()).iter().take(8).map(|b| format!("{b:02x}")).collect()
    }
}

/// The findings that stop the change: critical or high, and not already known at the base commit (the ratchet).
pub fn blocking<'a>(findings: &'a [Finding], known: &BTreeSet<String>) -> Vec<&'a Finding> {
    findings.iter().filter(|f| f.severity() <= Severity::High && !known.contains(&f.fingerprint())).collect()
}

/// The fingerprints in `tests/ux/baseline.json`: `{ "known": [{ "fingerprint": "...", "what": "..." }] }`.
pub fn known(root: &Path) -> Result<BTreeSet<String>, String> {
    let path = root.join("tests/ux/baseline.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(value["known"].as_array().into_iter().flatten().filter_map(|k| k["fingerprint"].as_str()).map(String::from).collect())
}

/// Writes the findings of a run, one record per fingerprint with every configuration where it showed.
pub fn write_findings(folder: &Path, scenario: &str, findings: &[Finding]) -> Result<(), String> {
    let mut records: Vec<Value> = Vec::new();
    for finding in findings {
        let print = finding.fingerprint();
        if let Some(at) = records.iter().position(|r| r["fingerprint"] == print.as_str()) {
            let configs = records[at]["configs"].as_array_mut().expect("configs is a list");
            if !configs.iter().any(|c| c == finding.config.as_str()) {
                configs.push(json!(finding.config));
            }
            continue;
        }
        records.push(json!({
            "fingerprint": print, "kind": finding.kind, "severity": format!("{:?}", finding.severity()), "screen": finding.screen,
            "surface": finding.surface, "element": finding.element, "configs": [finding.config], "metric": finding.metric,
        }));
    }
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let name: String = scenario.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let path = folder.join(format!("{name}.json"));
    std::fs::write(&path, serde_json::to_string_pretty(&records).expect("findings are JSON")).map_err(|e| format!("{}: {e}", path.display()))
}

// The frame clock.

/// Whether the hidden desktop gives the page real frames: at least 45 in the second a 1 s transition takes, a median
/// interval under 20 ms, and the `transitionend`. Anything else and motion results would mean nothing.
pub fn clock_problem(frames: &[f64], ended: bool) -> Option<String> {
    let mut gaps: Vec<f64> = frames.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.sort_by(f64::total_cmp);
    let median = gaps.get(gaps.len() / 2).copied().unwrap_or(f64::INFINITY);
    if frames.len() < 45 {
        Some(format!("{} frames in a 1 s transition, at least 45 wanted", frames.len()))
    } else if median >= 20.0 {
        Some(format!("median frame interval {median:.1} ms, under 20 wanted"))
    } else if !ended {
        Some("the transition never ended".into())
    } else {
        None
    }
}

// M1: a surface that jumps.

/// One frame of a surface: whether it is drawn, its box and its opacity, all effective (parents counted).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub t: f64,
    pub present: bool,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub o: f64,
}

pub fn samples(rows: &Value) -> Vec<Sample> {
    rows.as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let n = |i: usize| r.get(i).and_then(Value::as_f64);
            Some(Sample { t: n(0)?, present: n(1)? > 0.5, x: n(2)?, y: n(3)?, w: n(4)?, h: n(5)?, o: n(6)? })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Motion {
    /// The surface did not change between the first and the last frame.
    Still,
    /// It changed, through this many frames in between.
    Smooth(usize),
    /// It changed with no frame in between: a visible jump.
    Teleport(String),
}

/// An absent surface counts as invisible, in the place where it shows up, so appearing is a change of opacity only.
fn states(rows: &[Sample]) -> Vec<[f64; 5]> {
    let anchor = rows.iter().find(|s| s.present).or(rows.first());
    rows.iter()
        .map(|s| {
            let held = if s.present { s } else { anchor.unwrap_or(s) };
            [held.x, held.y, held.w, held.h, if s.present { s.o } else { 0.0 }]
        })
        .collect()
}

fn differs(a: &[f64; 5], b: &[f64; 5]) -> bool {
    (0..4).any(|i| (a[i] - b[i]).abs() > 1.0) || (a[4] - b[4]).abs() > 0.05
}

/// M1. A visible change (a move or a resize of 8 px or more, or an opacity change of 0.5 or more) with no frame in between.
pub fn teleport(rows: &[Sample]) -> Motion {
    let all = states(rows);
    let (Some(first), Some(last)) = (all.first(), all.last()) else { return Motion::Still };
    let moved = (0..4).map(|i| (first[i] - last[i]).abs()).fold(0.0, f64::max);
    let faded = (first[4] - last[4]).abs();
    if moved < 8.0 && faded < 0.5 {
        return Motion::Still;
    }
    // Frames at the start and end that already rest at the first or the last state do not count as in between.
    let between = all.iter().filter(|s| differs(s, first) && differs(s, last)).count();
    if between == 0 {
        Motion::Teleport(format!("moved {moved:.0} px, faded {faded:.2}, 0 frames in between of {}", all.len()))
    } else {
        Motion::Smooth(between)
    }
}

// M6: movement when the person asked for less.

/// An animation that started during the interaction: what it touches and how long it takes (delay included).
#[derive(Debug, Clone, PartialEq)]
pub struct Anim {
    pub target: String,
    pub props: Vec<String>,
    pub ms: f64,
    pub infinite: bool,
}

pub fn anims(rows: &Value) -> Vec<Anim> {
    rows.as_array()
        .into_iter()
        .flatten()
        .map(|a| Anim {
            target: a["target"].as_str().unwrap_or("?").into(),
            props: a["props"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(String::from)).collect(),
            ms: a["ms"].as_f64().unwrap_or(0.0),
            infinite: a["infinite"].as_bool().unwrap_or(false),
        })
        .collect()
}

/// M6. With reduced motion only opacity may change, and in 120 ms or less. With none, nothing may run at all.
pub fn reduced_problems(level: &str, found: &[Anim]) -> Vec<String> {
    found
        .iter()
        .filter_map(|a| {
            let moves: Vec<&str> = a.props.iter().map(String::as_str).filter(|p| *p != "opacity").collect();
            if level == "none" {
                Some(format!("{} runs under motion none ({})", a.target, a.props.join(", ")))
            } else if !moves.is_empty() {
                Some(format!("{} animates {} under reduced motion", a.target, moves.join(", ")))
            } else if a.ms > 120.0 {
                Some(format!("{} takes {:.0} ms under reduced motion, 120 wanted", a.target, a.ms))
            } else {
                None
            }
        })
        .collect()
}

// I1 to I5 and L4, L5: the composer and the end of the thread.

/// The composer's field after typing this many lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Typed {
    pub lines: u32,
    pub height: f64,
    pub scroll_height: f64,
    /// How far the send button's bottom edge is above the composer's bottom edge.
    pub bar_gap: f64,
    /// Whether the caret, at the end of the text, is inside the visible part of the field.
    pub caret_visible: bool,
}

/// I1 (grows), I2 (caps and scrolls), I4 (controls anchored). `line` is the line height, `stage` the height of the
/// stage the composer sits in; the cap must be 30% to 40% of it.
pub fn composer_problems(rows: &[Typed], line: f64, stage: f64) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let Some(last) = rows.last() else { return vec![("I1", "no rows".into())] };
    let cap = last.height;
    // The first line sits inside the field's minimum height; from the second on, each line adds one line height until
    // the cap, which is at least 30% of the stage.
    if let Some(two) = rows.iter().find(|r| r.lines == 2) {
        if let Some(one) = rows.iter().find(|r| r.lines == 1) {
            if two.height < one.height - 0.5 {
                out.push(("I1", format!("the field shrinks from {:.0} to {:.0} px at the second line", one.height, two.height)));
            }
        }
        for r in rows.iter().filter(|r| r.lines > 2) {
            let want = two.height + f64::from(r.lines - 2) * line;
            if want < 0.30 * stage && (r.height - want).abs() > 3.0 {
                out.push(("I1", format!("{} lines make the field {:.0} px, {want:.0} px wanted", r.lines, r.height)));
            }
        }
    }
    let share = cap / stage;
    if !(0.299..=0.401).contains(&share) {
        out.push(("I2", format!("the field stops at {cap:.0} px, {:.0}% of the {stage:.0} px stage, 30% to 40% wanted", share * 100.0)));
    }
    if last.scroll_height <= last.height + 1.0 {
        out.push(("I2", format!("{} lines do not scroll inside the field (content {:.0} px, field {:.0} px)", last.lines, last.scroll_height, last.height)));
    }
    if rows.iter().any(|r| !r.caret_visible) {
        out.push(("I2", "the caret leaves the visible part of the field".into()));
    }
    let first_gap = rows[0].bar_gap;
    if let Some(r) = rows.iter().find(|r| (r.bar_gap - first_gap).abs() > 1.0) {
        out.push(("I4", format!("the send button moved {:.0} px from the composer's bottom edge at {} lines", r.bar_gap - first_gap, r.lines)));
    }
    out
}

/// I3. After a send the field is back to one line, within one transition (400 ms at most).
pub fn shrink_problem(height_after: f64, min: f64) -> Option<String> {
    (height_after > min + 1.0).then(|| format!("{height_after:.0} px after sending, back to {min:.0} px wanted"))
}

/// L4. The last line of the last message must end at least 24 px above the composer.
pub fn gap_problem(last_line_bottom: f64, composer_top: f64) -> Option<String> {
    let gap = composer_top - last_line_bottom;
    (gap < 24.0).then(|| format!("the last line ends {gap:.0} px above the composer, 24 wanted"))
}

/// L5. At the end of a scroll area the last item must be whole: it ends inside the area, and what is drawn at its bottom
/// edge is the item itself, not something laid over it (`covered`).
pub fn cut_problem(last_item_bottom: f64, area_bottom: f64, covered: bool) -> Option<String> {
    if last_item_bottom > area_bottom + 0.5 {
        Some(format!("the last item ends {:.0} px past the edge of its scroll area", last_item_bottom - area_bottom))
    } else if covered {
        Some("the bottom edge of the last item is covered by something drawn over it".into())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(rows: &[(f64, bool, f64, f64)]) -> Vec<Sample> {
        // (time, present, x, opacity)
        rows.iter().map(|&(t, present, x, o)| Sample { t, present, x, y: 0.0, w: 300.0, h: 400.0, o }).collect()
    }

    #[test]
    fn a_fingerprint_ignores_the_configuration_and_the_numbers() {
        let a = Finding::new("M1", "chat.thread", "running", ".runcol", "1440 sala en", "0 frames");
        let b = Finding::new("M1", "chat.thread", "running", ".runcol", "720 manha de", "1 frame");
        let c = Finding::new("M1", "chat.thread", "side", ".side", "1440 sala en", "0 frames");
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_ne!(a.fingerprint(), c.fingerprint());
        assert_eq!(a.fingerprint().len(), 16);
    }

    #[test]
    fn only_critical_and_high_findings_that_are_new_block() {
        let t1 = Finding::new("T1", "chat.thread", "crumb", ".crumb", "", "");
        let m1 = Finding::new("M1", "chat.thread", "running", ".runcol", "", "");
        let l5 = Finding::new("L5", "chat.thread", "thread", ".thread", "", "");
        let all = vec![t1.clone(), m1.clone(), l5];
        assert_eq!(blocking(&all, &BTreeSet::new()).len(), 2, "a medium finding becomes a task, it does not block");
        let known = BTreeSet::from([m1.fingerprint()]);
        assert_eq!(blocking(&all, &known), vec![&t1], "a finding known at the base commit does not block");
        assert!(blocking(&[], &known).is_empty());
    }

    #[test]
    fn the_findings_file_has_one_record_per_fingerprint_with_every_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let a = Finding::new("T1", "chat.thread", "crumb", ".crumb", "1440 en", "x");
        let b = Finding::new("T1", "chat.thread", "crumb", ".crumb", "720 de", "x");
        write_findings(dir.path(), "text: a b", &[a.clone(), b, a]).unwrap();
        let text = std::fs::read_to_string(dir.path().join("text--a-b.json")).unwrap();
        let rows: Vec<Value> = serde_json::from_str(&text).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["configs"], json!(["1440 en", "720 de"]));
        assert_eq!(rows[0]["severity"], "Critical");
    }

    #[test]
    fn the_baseline_file_is_read_by_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("tests/ux")).unwrap();
        std::fs::write(dir.path().join("tests/ux/baseline.json"), r#"{ "known": [{ "fingerprint": "ab12", "what": "x" }] }"#).unwrap();
        assert_eq!(known(dir.path()).unwrap(), BTreeSet::from(["ab12".to_string()]));
        assert!(known(&dir.path().join("missing")).is_err());
    }

    #[test]
    fn a_clock_that_stalls_is_an_infrastructure_failure_not_a_pass() {
        let smooth: Vec<f64> = (0..60).map(|i| f64::from(i) * 16.7).collect();
        assert_eq!(clock_problem(&smooth, true), None);
        // Planted twins: a throttled desktop (10 fps), a stall (few frames) and a transition that never ends.
        let slow: Vec<f64> = (0..60).map(|i| f64::from(i) * 100.0).collect();
        assert!(clock_problem(&slow, true).unwrap().contains("median"));
        let few: Vec<f64> = (0..30).map(|i| f64::from(i) * 16.7).collect();
        assert!(clock_problem(&few, true).unwrap().contains("30 frames"));
        assert!(clock_problem(&smooth, false).unwrap().contains("never ended"));
        assert!(clock_problem(&[], true).is_some());
    }

    #[test]
    fn a_panel_that_slides_in_has_frames_in_between() {
        let slide = frames(&[(0.0, false, 0.0, 0.0), (16.0, true, 300.0, 0.1), (33.0, true, 200.0, 0.5), (50.0, true, 80.0, 0.9), (66.0, true, 0.0, 1.0), (83.0, true, 0.0, 1.0)]);
        assert_eq!(teleport(&slide), Motion::Smooth(3));
    }

    #[test]
    fn a_panel_that_appears_at_once_is_a_teleport() {
        // The planted twin of "transition: none": absent, then drawn whole in the next frame.
        let jump = frames(&[(0.0, false, 0.0, 0.0), (16.0, false, 0.0, 0.0), (33.0, true, 0.0, 1.0), (50.0, true, 0.0, 1.0)]);
        assert!(matches!(teleport(&jump), Motion::Teleport(_)));
        // The same on the way out: drawn, then gone with nothing in between.
        let gone = frames(&[(0.0, true, 0.0, 1.0), (16.0, true, 0.0, 1.0), (33.0, false, 0.0, 0.0)]);
        assert!(matches!(teleport(&gone), Motion::Teleport(_)));
        // A jump of position alone, 248 px.
        let moved = frames(&[(0.0, true, -248.0, 1.0), (16.0, true, 0.0, 1.0)]);
        assert!(matches!(teleport(&moved), Motion::Teleport(_)));
    }

    #[test]
    fn nothing_happening_is_not_a_teleport_and_small_changes_are_not_visible() {
        let still = frames(&[(0.0, true, 0.0, 1.0), (16.0, true, 0.0, 1.0)]);
        assert_eq!(teleport(&still), Motion::Still);
        let nudge = frames(&[(0.0, true, 0.0, 1.0), (16.0, true, 6.0, 1.0)]);
        assert_eq!(teleport(&nudge), Motion::Still);
        assert_eq!(teleport(&[]), Motion::Still);
    }

    #[test]
    fn samples_and_animations_are_read_from_the_probe() {
        let rows = json!([[0, 0, 0, 0, 0, 0, 0], [16, 1, 10.0, 20.0, 300.0, 400.0, 0.5]]);
        let read = samples(&rows);
        assert_eq!(read.len(), 2);
        assert!(!read[0].present && read[1].present && read[1].o == 0.5);
        assert_eq!(anims(&json!([{ "target": "div.runcol", "props": ["transform", "opacity"], "ms": 220, "infinite": false }]))[0].props, ["transform", "opacity"]);
    }

    #[test]
    fn reduced_motion_allows_a_short_fade_and_nothing_else() {
        let fade = Anim { target: "div.ask".into(), props: vec!["opacity".into()], ms: 100.0, infinite: false };
        let slide = Anim { target: "aside.runcol".into(), props: vec!["transform".into(), "opacity".into()], ms: 220.0, infinite: false };
        let slow = Anim { target: "div.ask".into(), props: vec!["opacity".into()], ms: 400.0, infinite: false };
        assert!(reduced_problems("reduced", &[fade.clone()]).is_empty());
        assert_eq!(reduced_problems("reduced", &[slide.clone()]).len(), 1, "planted twin: a slide under reduced motion");
        assert_eq!(reduced_problems("reduced", &[slow]).len(), 1);
        assert_eq!(reduced_problems("none", &[fade]).len(), 1, "even a fade is motion under none");
        assert!(reduced_problems("none", &[]).is_empty());
    }

    fn typed(lines: u32, height: f64, scroll_height: f64) -> Typed {
        Typed { lines, height, scroll_height, bar_gap: 10.0, caret_visible: true }
    }

    /// A composer like the shipped one: 60 px with one line, 8 px more for the second, 24 px for each after, capped at 360.
    fn working() -> Vec<Typed> {
        vec![typed(1, 60.0, 44.0), typed(2, 68.0, 68.0), typed(5, 140.0, 140.0), typed(10, 260.0, 260.0), typed(40, 360.0, 980.0)]
    }

    #[test]
    fn a_composer_that_grows_caps_and_scrolls_passes() {
        assert_eq!(composer_problems(&working(), 24.0, 1000.0), vec![]);
    }

    #[test]
    fn a_composer_stuck_at_one_height_is_found() {
        // The planted twin of a fixed composer height: it never grows, and so never scrolls to a cap either.
        let fixed: Vec<Typed> = [1, 2, 5, 10, 40].iter().map(|&n| typed(n, 60.0, 60.0 + f64::from(n) * 4.0)).collect();
        let problems = composer_problems(&fixed, 24.0, 1000.0);
        assert!(problems.iter().any(|(id, _)| *id == "I1"), "{problems:?}");
        assert!(problems.iter().any(|(id, _)| *id == "I2"), "{problems:?}");
    }

    #[test]
    fn a_composer_that_never_stops_or_hides_the_caret_or_moves_its_button_is_found() {
        let mut rows = working();
        rows[4].height = 900.0;
        rows[4].scroll_height = 900.0;
        assert!(composer_problems(&rows, 24.0, 1000.0).iter().any(|(id, m)| *id == "I2" && m.contains("30% to 40%")));
        let mut rows = working();
        rows[4].caret_visible = false;
        assert!(composer_problems(&rows, 24.0, 1000.0).iter().any(|(id, m)| *id == "I2" && m.contains("caret")));
        let mut rows = working();
        rows[3].bar_gap = 40.0;
        assert!(composer_problems(&rows, 24.0, 1000.0).iter().any(|(id, _)| *id == "I4"));
    }

    #[test]
    fn the_field_shrinks_after_a_send_and_the_thread_keeps_its_gap() {
        assert_eq!(shrink_problem(60.0, 60.0), None);
        assert!(shrink_problem(260.0, 60.0).is_some());
        assert_eq!(gap_problem(700.0, 764.0), None);
        assert_eq!(gap_problem(700.0, 724.0), None);
        assert!(gap_problem(700.0, 710.0).unwrap().contains("10 px"), "planted twin: no padding under the last message");
        assert!(gap_problem(720.0, 700.0).is_some(), "text under the composer");
        assert_eq!(cut_problem(700.0, 700.0, false), None);
        assert!(cut_problem(712.0, 700.0, false).is_some());
        assert!(cut_problem(700.0, 700.0, true).unwrap().contains("covered"), "planted twin: the composer lies over the last item");
    }
}
