//! Read-only review of exactly one saved turn, independent of the current working folder.

use super::*;
use crate::changes::FileChange;

const TEXT_BYTES: u64 = 2 * 1024 * 1024;
const TEXT_LINES: usize = 20_000;
const DIFF_WORK: usize = 1_000_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerChanges {
    pub files: Vec<FileChange>,
    pub total: usize,
    pub coverage: Coverage,
    pub sealed: bool,
    pub shared: bool,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerChangePatch {
    pub lines: Vec<String>,
    pub total: usize,
    pub binary: bool,
    pub too_large: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<&'static str>,
}

impl History {
    fn review_point(&self, conversation: &str, message: &str) -> Result<&Point, String> {
        self.points
            .values()
            .filter(|point| point.conversation == conversation && point.info.message_id == message)
            .max_by_key(|point| point.sequence)
            .ok_or_else(|| "error.rewind.unavailable".into())
    }

    pub(crate) fn review_matches(&self, conversation: &str, message: &str, id: &str) -> bool {
        self.review_point(conversation, message).is_ok_and(|point| point.info.id == id)
    }

    /// Lists saved metadata only. No Git process, project path or object contents are read.
    pub fn review(&self, conversation: &str, message: &str, offset: usize) -> Result<AnswerChanges, String> {
        let point = self.review_point(conversation, message)?;
        let sealed = point.info.sealed && point.after.is_some() && !point.review_pending;
        let shared = point.unknown_author
            || point.review_shared.unwrap_or_else(|| {
                point.after.as_ref().is_some_and(|after| {
                    self.points.values().any(|other| {
                        other.info.id != point.info.id
                            && other.folder == point.folder
                            && other.after.as_ref().is_some_and(|next| same_snapshot(after, next))
                    })
                })
            });
        let mut coverage = point.info.coverage.clone();
        coverage.incomplete |= !sealed;
        coverage.unavailable |= !sealed && !point.active && !point.review_pending;
        if !sealed {
            return Ok(AnswerChanges { files: vec![], total: 0, coverage, sealed, shared });
        }
        let files = saved_changes(point);
        Ok(AnswerChanges {
            total: files.len(),
            coverage,
            sealed,
            shared,
            files: files
                .into_iter()
                .skip(offset)
                .take(50)
                .map(|(path, before, after)| FileChange {
                    path: path.into(),
                    added: 0,
                    removed: 0,
                    binary: false,
                    counts_known: false,
                    too_large: before.into_iter().chain(after).any(|image| image.bytes > TEXT_BYTES),
                    status: if before.is_none() {
                        "added"
                    } else if after.is_none() {
                        "deleted"
                    } else {
                        "modified"
                    }
                    .into(),
                })
                .collect(),
        })
    }

    /// Diffs exactly this turn's immutable images, never the later range used by restore.
    pub fn review_patch(&self, conversation: &str, message: &str, path: &str, offset: usize) -> Result<AnswerChangePatch, String> {
        let point = self.review_point(conversation, message)?;
        if !point.info.sealed || point.after.is_none() || point.review_pending {
            return Err("error.rewind.unavailable".into());
        }
        let (_, before, after) = saved_changes(point).into_iter().find(|(name, _, _)| *name == path).ok_or("error.rewind.unavailable")?;
        let notice = match (before, after) {
            (Some(old), Some(new)) if old.same_bytes(new) && old.protection != new.protection => Some("answerChanges.properties"),
            (None, Some(new)) if new.bytes == 0 => Some("answerChanges.emptyAdded"),
            (Some(old), None) if old.bytes == 0 => Some("answerChanges.emptyDeleted"),
            _ => None,
        };
        if notice.is_some() {
            return Ok(AnswerChangePatch { notice, ..Default::default() });
        }
        if before.into_iter().chain(after).any(|image| image.bytes > TEXT_BYTES) {
            return Ok(AnswerChangePatch { too_large: true, ..Default::default() });
        }
        let read = |image: Option<&Image>| image.map_or_else(|| Ok(vec![]), |image| self.blob(&point.folder, image));
        let (old, new) = (read(before)?, read(after)?);
        let (Ok(old), Ok(new)) = (std::str::from_utf8(&old), std::str::from_utf8(&new)) else {
            return Ok(AnswerChangePatch { binary: true, ..Default::default() });
        };
        if old.contains('\0') || new.contains('\0') {
            return Ok(AnswerChangePatch { binary: true, ..Default::default() });
        }
        if old.lines().count() > TEXT_LINES
            || new.lines().count() > TEXT_LINES
            || old.lines().chain(new.lines()).any(|line| line.len() > 16_384)
        {
            return Ok(AnswerChangePatch { too_large: true, ..Default::default() });
        }
        let Some(lines) = saved_diff(path, before.is_some(), after.is_some(), old, new) else {
            return Ok(AnswerChangePatch { too_large: true, ..Default::default() });
        };
        let total = lines.len();
        let mut bytes = 0;
        let lines = lines
            .into_iter()
            .skip(offset)
            .take(200)
            .take_while(|line| {
                bytes += line.len();
                bytes <= 128 * 1024
            })
            .collect();
        Ok(AnswerChangePatch { lines, total, ..Default::default() })
    }
}

fn same_snapshot(a: &Snapshot, b: &Snapshot) -> bool {
    a.complete == b.complete
        && a.absent == b.absent
        && a.skipped == b.skipped
        && a.excluded == b.excluded
        && a.files.len() == b.files.len()
        && a.files.iter().zip(&b.files).all(|((a_name, a), (b_name, b))| {
            a_name == b_name && a.same_bytes(b) && a.protection == b.protection && a.identity == b.identity
        })
}

fn saved_changes(point: &Point) -> Vec<(&str, Option<&Image>, Option<&Image>)> {
    let Some(after) = &point.after else {
        return vec![];
    };
    let paths: BTreeSet<_> = point.before.files.keys().chain(after.files.keys()).collect();
    paths
        .into_iter()
        .filter_map(|path| {
            if !crate::changes::safe(path) {
                return None;
            }
            let (before, after) = (point.before.known(path)?, after.known(path)?);
            if equal(before, after) && before.zip(after).is_none_or(|(a, b)| a.protection == b.protection) {
                return None;
            }
            Some((path.as_str(), before, after))
        })
        .collect()
}

fn saved_diff(path: &str, before: bool, after: bool, old: &str, new: &str) -> Option<Vec<String>> {
    let old: Vec<_> = old.split_inclusive('\n').collect();
    let new: Vec<_> = new.split_inclusive('\n').collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..].iter().rev().zip(new[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
    let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
    let edits = shortest_edits(&old[prefix..old_end], &new[prefix..new_end], DIFF_WORK.saturating_sub(old.len() + new.len()))?;
    let start = prefix.saturating_sub(3);
    let old_to = (old_end + 3).min(old.len());
    let edits: Vec<_> = old[start..prefix]
        .iter()
        .map(|line| (' ', *line))
        .chain(edits)
        .chain(old[old_end..old_to].iter().map(|line| (' ', *line)))
        .collect();
    let changed: Vec<_> = edits.iter().enumerate().filter_map(|(index, (sign, _))| (*sign != ' ').then_some(index)).collect();
    if changed.is_empty() {
        return Some(vec![]);
    }
    let mut hunks = vec![];
    let (mut first, mut last) = (changed[0], changed[0]);
    for &index in &changed[1..] {
        if index - last > 7 {
            hunks.push((first.saturating_sub(3), (last + 4).min(edits.len())));
            first = index;
        }
        last = index;
    }
    hunks.push((first.saturating_sub(3), (last + 4).min(edits.len())));
    let label = |name: String| if name.chars().any(char::is_control) { format!("{name:?}") } else { name };
    let mut lines = vec![
        format!("--- {}", label(if before { format!("a/{path}") } else { "/dev/null".into() })),
        format!("+++ {}", label(if after { format!("b/{path}") } else { "/dev/null".into() })),
    ];
    let (mut position, mut old_line, mut new_line) = (0, start + 1, start + 1);
    for (from, to) in hunks {
        for (sign, _) in &edits[position..from] {
            old_line += usize::from(*sign != '+');
            new_line += usize::from(*sign != '-');
        }
        let old_count = edits[from..to].iter().filter(|(sign, _)| *sign != '+').count();
        let new_count = edits[from..to].iter().filter(|(sign, _)| *sign != '-').count();
        lines.push(format!(
            "@@ -{},{} +{},{} @@",
            old_line - usize::from(old_count == 0),
            old_count,
            new_line - usize::from(new_count == 0),
            new_count
        ));
        for (sign, text) in &edits[from..to] {
            lines.push(format!("{sign}{}", text.trim_end_matches('\n')));
            if !text.ends_with('\n') {
                lines.push("\\ No newline at end of file".into());
            }
            old_line += usize::from(*sign != '+');
            new_line += usize::from(*sign != '-');
        }
        position = to;
    }
    Some(lines)
}

fn shortest_edits<'a>(old: &[&'a str], new: &[&'a str], budget: usize) -> Option<Vec<(char, &'a str)>> {
    if old.is_empty() {
        return Some(new.iter().map(|line| ('+', *line)).collect());
    }
    if new.is_empty() {
        return Some(old.iter().map(|line| ('-', *line)).collect());
    }
    let mut trace: Vec<Vec<i32>> = vec![];
    let mut work = 0;
    for depth in 0..=old.len() + new.len() {
        let mut row = vec![0; depth + 1];
        for index in 0..=depth {
            work += 1;
            if work > budget {
                return None;
            }
            let diagonal = 2 * index as i32 - depth as i32;
            let mut x = if depth == 0 {
                0
            } else {
                let previous = &trace[depth - 1];
                if index == 0 || (index < depth && previous[index - 1] < previous[index]) {
                    previous[index]
                } else {
                    previous[index - 1] + 1
                }
            };
            let mut y = x - diagonal;
            while x < old.len() as i32 && y < new.len() as i32 {
                work += 1;
                if work > budget {
                    return None;
                }
                if old[x as usize] != new[y as usize] {
                    break;
                }
                x += 1;
                y += 1;
            }
            row[index] = x;
            if x >= old.len() as i32 && y >= new.len() as i32 {
                trace.push(row);
                let (mut x, mut y) = (old.len(), new.len());
                let mut edits = Vec::with_capacity(x + y);
                for depth in (1..trace.len()).rev() {
                    let index = ((x as i32 - y as i32 + depth as i32) / 2) as usize;
                    let previous = &trace[depth - 1];
                    let insert = index == 0 || (index < depth && previous[index - 1] < previous[index]);
                    let from_x = previous[if insert { index } else { index - 1 }] as usize;
                    let from_diagonal = x as i32 - y as i32 + if insert { 1 } else { -1 };
                    let from_y = (from_x as i32 - from_diagonal) as usize;
                    while x > from_x && y > from_y {
                        x -= 1;
                        y -= 1;
                        edits.push((' ', old[x]));
                    }
                    if insert {
                        edits.push(('+', new[from_y]));
                    } else {
                        edits.push(('-', old[from_x]));
                    }
                    (x, y) = (from_x, from_y);
                }
                while x > 0 && y > 0 {
                    x -= 1;
                    y -= 1;
                    edits.push((' ', old[x]));
                }
                edits.reverse();
                return Some(edits);
            }
        }
        trace.push(row);
    }
    None
}

#[cfg(test)]
mod tests;
