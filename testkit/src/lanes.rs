//! Lane claims (R154): a worktree on a branch says which product-map areas and which files it will change, in
//! `.faber-lane` at its root, before it changes one. The runner stops a run whose changes leave the claim, or whose claim
//! overlaps another worktree's, so two writers never touch the same files and a reviewer knows the blast radius.
//!
//! ```text
//! area: Settings screen
//! own: ui/Settings.tsx
//! share: ui/locales/
//! ```
//!
//! `own` is the lane's alone; `share` is a file several lanes may append to. A path claims itself and everything under
//! it. `area: none` is work no person sees. The main checkout and detached worktrees (Faber's own missions, test
//! workspaces) need no claim, and another lane's claim that nobody saved for a day is stale and ignored.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const FILE: &str = ".faber-lane";
const STALE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Claim {
    pub areas: Vec<String>,
    /// Each claimed path, and whether it is shared.
    pub paths: Vec<(String, bool)>,
}

pub fn parse(text: &str) -> Result<Claim, String> {
    let mut claim = Claim::default();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let wrong = || format!("`{line}`: write `area:`, `own:` or `share:` and a value");
        let (key, value) = line.split_once(':').map(|(k, v)| (k.trim(), v.trim())).filter(|(_, v)| !v.is_empty()).ok_or_else(wrong)?;
        let path = value.replace('\\', "/").trim_start_matches("./").trim_end_matches('/').to_string();
        match key {
            "area" => claim.areas.push(value.to_string()),
            "own" | "share"
                if path.is_empty() || path.starts_with('/') || path.contains(':') || path.split('/').any(|p| p == ".." || p == ".") =>
            {
                return Err(format!("`{line}`: claim files or folders inside the repository, never all of it"));
            }
            "own" | "share" => claim.paths.push((path, key == "share")),
            _ => return Err(wrong()),
        }
    }
    if claim.areas.is_empty() {
        return Err("name at least one area (`area: none` for work no person sees)".into());
    }
    Ok(claim)
}

fn covers(claimed: &str, path: &str) -> bool {
    // Windows and macOS ignore case in file names, so `ui/settings.tsx` is `ui/Settings.tsx`.
    let (claimed, path) = (claimed.to_lowercase(), path.to_lowercase());
    path == claimed || path.strip_prefix(&claimed).is_some_and(|rest| rest.starts_with('/'))
}

/// The names an area may take: every heading and the first cell of every table row of the product map, in lower case.
pub fn map_areas(map: &str) -> Vec<String> {
    map.lines()
        .filter_map(|l| l.strip_prefix('#').map(|h| h.trim_start_matches('#')).or_else(|| l.strip_prefix('|')?.split('|').next()))
        .map(|name| name.trim().trim_matches('`').to_lowercase())
        .filter(|name| !name.is_empty() && !name.starts_with(":-"))
        .collect()
}

/// What is wrong with this lane's claim, given the files it changed, the map's areas and the other live lanes' claims
/// (each with the folder it lives in). Empty when it holds.
pub fn problems(mine: &Claim, changed: &[String], areas: &[String], others: &[(String, Claim)]) -> Vec<String> {
    let mut found = Vec::new();
    for area in mine.areas.iter().filter(|a| !a.eq_ignore_ascii_case("none") && !areas.contains(&a.to_lowercase())) {
        found.push(format!("area `{area}` is not in docs/product-map.md: use a heading or the first cell of a row, or `none`"));
    }
    let loose: Vec<&str> = changed.iter().filter(|f| !mine.paths.iter().any(|(p, _)| covers(p, f))).map(String::as_str).collect();
    if !loose.is_empty() {
        found.push(format!("changed but not claimed: {}. Claim them in {FILE} as `own:` or `share:`", loose.join(", ")));
    }
    for (place, other) in others {
        for (path, shared) in &mine.paths {
            for (theirs, their_share) in other.paths.iter().filter(|(t, s)| (covers(path, t) || covers(t, path)) && !(*shared && *s)) {
                let kind = if *their_share { "share" } else { "own" };
                found.push(format!(
                    "`{path}` overlaps `{kind}: {theirs}` of the lane in {place}. Two writers never touch the same files: wait for it to \
                     land, or ask the lead session"
                ));
            }
        }
    }
    found
}

/// The worktrees in `git worktree list --porcelain`, the main checkout first, each with whether it is on a branch.
pub fn worktrees(porcelain: &str) -> Vec<(PathBuf, bool)> {
    let mut list: Vec<(PathBuf, bool)> = Vec::new();
    for line in porcelain.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            list.push((PathBuf::from(path), false));
        } else if let Some(last) = list.last_mut().filter(|_| line.starts_with("branch ")) {
            last.1 = true;
        }
    }
    list
}

/// A worktree's claim and when it was last saved; `None` when it has none.
fn load(dir: &Path) -> Option<(SystemTime, Result<Claim, String>)> {
    let path = dir.join(FILE);
    let saved = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
    Some((saved, std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| parse(&text))))
}

/// The lane check of a run in `root`: empty when it holds, and always for the main checkout and detached worktrees.
pub fn check(root: &Path, porcelain: &str, changed: &[String]) -> Vec<String> {
    let here = std::fs::canonicalize(root).ok();
    let list = worktrees(porcelain);
    let Some(index) = list.iter().position(|(path, _)| std::fs::canonicalize(path).ok() == here) else { return Vec::new() };
    if index == 0 || !list[index].1 {
        return Vec::new();
    }
    let mine = match load(root) {
        None if changed.is_empty() => return Vec::new(),
        None => {
            return vec![format!(
                "this worktree changes files but claims none: write {FILE} at its root first (docs/design.md#lane-claims)"
            )];
        }
        Some((_, Err(e))) => return vec![format!("{FILE}: {e}")],
        Some((_, Ok(claim))) => claim,
    };
    let now = SystemTime::now();
    let others: Vec<(String, Claim)> = list
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != index)
        .filter_map(|(_, (path, _))| {
            let (saved, claim) = load(path)?;
            let fresh = now.duration_since(saved).map_or(true, |age| age < STALE);
            Some((path.display().to_string(), claim.ok().filter(|_| fresh)?))
        })
        .collect();
    let map = std::fs::read_to_string(root.join("docs/product-map.md")).unwrap_or_default();
    problems(&mine, changed, &map_areas(&map), &others)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim(text: &str) -> Claim {
        parse(text).unwrap()
    }
    fn files(list: &[&str]) -> Vec<String> {
        list.iter().map(|f| f.to_string()).collect()
    }

    #[test]
    fn a_claim_has_areas_and_paths_and_refuses_anything_vague() {
        let c = claim("# lane x\narea: Settings screen\nown: ui/Settings.tsx\nshare: ui\\locales/\n");
        assert_eq!(c.areas, ["Settings screen"]);
        assert_eq!(c.paths, [("ui/Settings.tsx".to_string(), false), ("ui/locales".to_string(), true)]);
        for bad in [
            "own: ui/x.tsx",
            "area: none\nown: .",
            "area: none\nown: /",
            "area: none\nshare: ../x",
            "area: none\nown: C:/x",
            "area:",
            "area: none\nmine: x",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn every_changed_file_needs_a_claim_and_a_folder_claims_what_is_under_it() {
        let c = claim("area: none\nown: testkit/src\nshare: docs/decisions.md");
        assert!(problems(&c, &files(&["testkit/src/lanes.rs", "docs/decisions.md"]), &[], &[]).is_empty());
        let found = problems(&c, &files(&["testkit/srcx.rs", "AGENTS.md"]), &[], &[]);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("testkit/srcx.rs, AGENTS.md"), "{found:?}");
        assert!(problems(&claim("area: none\nown: ui/settings.tsx"), &files(&["UI/Settings.tsx"]), &[], &[]).is_empty(), "case");
    }

    #[test]
    fn an_area_must_be_in_the_product_map() {
        let areas = map_areas("# Product map\n## Settings screen\n| Surface | Shows |\n| :--- | :--- |\n| `Sidebar` | x |\n");
        assert_eq!(areas, ["product map", "settings screen", "surface", "sidebar"]);
        let c = claim("area: sidebar\narea: None\narea: Toolbox\nown: ui/Sidebar.tsx");
        let found = problems(&c, &[], &areas, &[]);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("`Toolbox`"), "{found:?}");
    }

    #[test]
    fn two_lanes_may_share_a_file_but_never_own_the_same_one() {
        let mine = claim("area: none\nown: engine/src/usage.rs\nshare: ui/locales");
        let place = |text: &str| ("E:/p/faber-wt-other".to_string(), claim(text));
        assert!(problems(&mine, &[], &[], &[place("area: none\nshare: ui/locales/en.ts\nown: engine/src/route.rs")]).is_empty());
        for theirs in [
            "area: none\nown: engine/src",
            "area: none\nshare: engine/src/usage.rs",
            "area: none\nown: ui/locales/en.ts",
            "area: none\nown: Engine/Src/Usage.rs",
        ] {
            let found = problems(&mine, &[], &[], &[place(theirs)]);
            assert_eq!(found.len(), 1, "{theirs}");
            assert!(found[0].contains("E:/p/faber-wt-other"), "{found:?}");
        }
    }

    #[test]
    fn worktrees_know_which_one_is_on_a_branch() {
        let porcelain = "worktree E:/p/faber\nHEAD 1\nbranch refs/heads/phase-1-base\n\nworktree E:/p/faber-wt-a\nHEAD 2\nbranch refs/heads/a\n\n\
                         worktree E:/t/tree\nHEAD 3\ndetached\n";
        assert_eq!(
            worktrees(porcelain),
            [(PathBuf::from("E:/p/faber"), true), (PathBuf::from("E:/p/faber-wt-a"), true), (PathBuf::from("E:/t/tree"), false)]
        );
    }
}
