//! A file a reply names (`src/app.ts:42`, a link to it), opened read only at that line (parity P2). Only a plain file inside
//! the conversation's folder is read, through the same checks and handles as the changes view (`changes::read`): no link,
//! no path outside the folder, no protected file.

use crate::changes::{FILE_BYTES, PATCH_LINES};
use serde::Serialize;
use std::path::Path;

/// The most lines one view holds, like a patch: a longer file shows the lines around the one asked for.
pub const LINES: usize = PATCH_LINES;

#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileView {
    /// The file inside the folder, with `/` between its parts; empty when the reference was refused.
    pub path: String,
    /// The line asked for, and the last one of a range, when they are in the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<usize>,
    /// The number of the first line of `text`, from 1.
    pub first: usize,
    pub text: String,
    /// How many lines the file has.
    pub total: usize,
    pub too_large: bool,
    pub binary: bool,
    /// An `error.*` key: outside the folder, missing, or unreadable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl FileView {
    pub fn failed(key: &str) -> Self {
        Self { error: Some(key.to_string()), ..Self::default() }
    }
}

/// A reference split into the file inside `root` and its line.
#[derive(Debug, PartialEq, Eq)]
pub struct FileRef {
    pub path: String,
    pub line: Option<usize>,
    pub end: Option<usize>,
}

/// The digits at the end of `s`, and what comes before them.
fn trailing_number(s: &str) -> Option<(&str, usize)> {
    let digits = s.len() - s.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 || digits > 9 {
        return None;
    }
    let at = s.len() - digits;
    Some((&s[..at], s[at..].parse().ok()?))
}

/// `path:42`, `path:42:7` (a column), `path:10-20`, `path#L42` and `path#L10-L20`: the path and its line and range end.
fn split_line(s: &str) -> (&str, Option<usize>, Option<usize>) {
    if let Some(at) = s.rfind("#L") {
        let (path, tail) = (&s[..at], &s[at + 2..]);
        let (start, end) = tail.split_once('-').map_or((tail, None), |(a, b)| (a, Some(b.trim_start_matches('L'))));
        if let Ok(line) = start.parse() {
            return (path, Some(line), end.and_then(|e| e.parse().ok()));
        }
    }
    let Some((rest, last)) = trailing_number(s) else { return (s, None, None) };
    if let Some(head) = rest.strip_suffix('-').or_else(|| rest.strip_suffix(':')) {
        if let Some((before, first)) = trailing_number(head).filter(|(before, _)| before.ends_with(':')) {
            let path = &before[..before.len() - 1];
            return if rest.ends_with('-') { (path, Some(first), Some(last)) } else { (path, Some(first), None) };
        }
        if rest.ends_with(':') {
            return (head, Some(last), None);
        }
    }
    (s, None, None)
}

/// `%20` and the like, from a link's address. Text that is not valid UTF-8 once decoded stays as it was.
fn unescape(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

fn absolute(path: &str) -> bool {
    let b = path.as_bytes();
    path.starts_with('/') || (b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'/')
}

/// `root` as a reply would write it: `/` between parts, without Windows' `\\?\` prefix or a trailing slash.
fn written(root: &Path) -> String {
    let root = root.to_string_lossy().replace('\\', "/");
    let root = root.strip_prefix("//?/").unwrap_or(&root);
    root.trim_end_matches('/').to_string()
}

/// The file `raw` names inside `root`, or None when it points anywhere else or at a protected file. A relative path is read
/// from `root`; an absolute one only when it lies under `root` (Windows compares letters without case, like the disk).
pub fn parse(root: &Path, raw: &str) -> Option<FileRef> {
    let raw = raw.trim().trim_matches(['`', '"', '\'']);
    let raw = raw.strip_prefix("file://").map_or(raw, |rest| {
        let b = rest.as_bytes();
        if b.len() > 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' { &rest[1..] } else { rest }
    });
    let decoded = unescape(raw);
    let (path, line, end) = split_line(&decoded);
    let path = path.replace('\\', "/");
    let relative = if absolute(&path) {
        let root = written(root);
        let head = path.get(..root.len())?;
        let same = if cfg!(windows) { head.eq_ignore_ascii_case(&root) } else { head == root };
        if !same || path.as_bytes().get(root.len()) != Some(&b'/') {
            return None;
        }
        path[root.len() + 1..].to_string()
    } else {
        let mut rest = path.as_str();
        while let Some(after) = rest.strip_prefix("./") {
            rest = after;
        }
        rest.to_string()
    };
    // Windows reads `.GIT/config` and `.env.` as `.git/config` and `.env`, and `GIT~1` as the short name of `.git`: the checks see
    // the name as the disk will, and a short name is never followed.
    let folded = relative.to_ascii_lowercase();
    let short = |part: &str| part.split('~').skip(1).any(|after| after.starts_with(|c: char| c.is_ascii_digit()));
    if folded.split('/').any(|part| part.ends_with(['.', ' ']) || short(part))
        || !crate::changes::safe(&relative)
        || !crate::changes::safe(&folded)
    {
        return None;
    }
    let line = line.filter(|n| *n > 0);
    Some(FileRef { path: relative, line, end: line.and(end) })
}

/// The file `raw` names, at most `LINES` lines of it around its line.
pub fn view(root: &Path, raw: &str) -> FileView {
    let Some(at) = parse(root, raw) else { return FileView::failed("error.file.outside") };
    let mut view = FileView { path: at.path.clone(), first: 1, ..FileView::default() };
    let limit = FILE_BYTES as usize;
    let bytes = match crate::changes::read(root, &at.path, limit) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return FileView { error: Some("error.file.missing".into()), ..view },
        Err(_) => return FileView { error: Some("error.file.read".into()), ..view },
    };
    if bytes.len() > limit {
        return FileView { too_large: true, ..view };
    }
    if bytes[..bytes.len().min(8_000)].contains(&0) {
        return FileView { binary: true, ..view };
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len();
    let line = at.line.filter(|n| *n <= total);
    let end = line.and_then(|first| at.end.filter(|e| *e > first)).map(|e| e.min(total));
    let start = match line {
        Some(n) if total > LINES => (n - 1).saturating_sub(LINES / 2).min(total - LINES),
        _ => 0,
    };
    view.first = start + 1;
    view.text = lines[start..(start + LINES).min(total)].join("\n");
    FileView { total, line, end, ..view }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(path: &str, line: Option<usize>, end: Option<usize>) -> Option<FileRef> {
        Some(FileRef { path: path.into(), line, end })
    }

    #[test]
    fn a_reference_gives_its_file_and_line_in_every_form_the_agents_write() {
        let root = Path::new("/work/app");
        assert_eq!(parse(root, "src/app.ts:42"), at("src/app.ts", Some(42), None));
        assert_eq!(parse(root, "`src/app.ts:42:7`"), at("src/app.ts", Some(42), None), "a column is not a range");
        assert_eq!(parse(root, "src/app.ts:10-20"), at("src/app.ts", Some(10), Some(20)));
        assert_eq!(parse(root, "src/app.ts#L5-L9"), at("src/app.ts", Some(5), Some(9)));
        assert_eq!(parse(root, "src/app.ts#L5"), at("src/app.ts", Some(5), None));
        assert_eq!(parse(root, "./README.md"), at("README.md", None, None));
        assert_eq!(parse(root, "app.ts:0"), at("app.ts", None, None), "lines start at 1");
    }

    #[test]
    fn spaces_escapes_and_windows_paths_name_the_same_file() {
        let root = Path::new("/work/app");
        assert_eq!(parse(root, "docs/my notes.md:3"), at("docs/my notes.md", Some(3), None));
        assert_eq!(parse(root, "docs/my%20notes.md"), at("docs/my notes.md", None, None));
        assert_eq!(parse(root, r"src\ui\app.ts:12"), at("src/ui/app.ts", Some(12), None));
        assert_eq!(parse(root, "/work/app/src/app.ts:4"), at("src/app.ts", Some(4), None));
        assert_eq!(parse(root, "file:///work/app/src/app.ts"), at("src/app.ts", None, None));
        let windows = Path::new(r"E:\Projects\app");
        assert_eq!(parse(windows, r"E:\Projects\app\src\app.ts:42"), at("src/app.ts", Some(42), None));
        assert_eq!(parse(windows, "file:///E:/Projects/app/src/app.ts#L42"), at("src/app.ts", Some(42), None));
        assert_eq!(parse(Path::new(r"\\?\E:\Projects\app"), "E:/Projects/app/a b.ts"), at("a b.ts", None, None));
        if cfg!(windows) {
            assert_eq!(parse(windows, r"e:\projects\APP\src\app.ts"), at("src/app.ts", None, None));
        }
    }

    #[test]
    fn a_path_outside_the_folder_or_to_a_protected_file_is_refused() {
        let root = Path::new(r"E:\Projects\app");
        for raw in [
            "../secret.txt",
            "src/../../secret.txt",
            r"E:\Projects\other\a.ts",
            r"E:\Projects\app-old\a.ts",
            r"E:\Projects\app",
            "/etc/passwd",
            "C:/Windows/win.ini",
            ".git/config",
            ".GIT/config",
            ".git./config",
            ".env",
            ".ENV",
            "src/.env.",
            "GIT~1/config",
            "ENV~1",
            "notes.txt:secret",
            "src /app.ts",
            "src//app.ts",
            "",
        ] {
            assert_eq!(parse(root, raw), None, "{raw} must be refused");
        }
    }

    #[test]
    fn a_view_holds_the_lines_around_its_line_and_never_more_than_a_patch() {
        let folder = std::env::temp_dir().join(format!("faber-fileview-{}", std::process::id()));
        std::fs::create_dir_all(folder.join("src")).unwrap();
        let text: String = (1..=LINES * 3).map(|i| format!("line {i}\r\n")).collect();
        std::fs::write(folder.join("src/long.txt"), text).unwrap();
        std::fs::write(folder.join("src/short.txt"), "one\ntwo\nthree").unwrap();
        std::fs::write(folder.join("src/blob.bin"), [1u8, 0, 2]).unwrap();

        let long = view(&folder, "src/long.txt:6000");
        assert_eq!((long.line, long.total, long.first), (Some(6000), LINES * 3, 6000 - LINES / 2));
        assert_eq!(long.text.lines().count(), LINES);
        assert_eq!(long.text.lines().nth(LINES / 2), Some("line 6000"), "the line asked for sits in the middle, without its \\r");
        let end = view(&folder, "src/long.txt:11999");
        assert_eq!(end.first, LINES * 2 + 1, "near the end the view stops at the last line");

        let short = view(&folder, "src/short.txt:2-9");
        assert_eq!((short.first, short.line, short.end, short.text.as_str()), (1, Some(2), Some(3), "one\ntwo\nthree"));
        assert_eq!(view(&folder, "src/short.txt:9").line, None, "a line past the end marks nothing");
        assert!(view(&folder, "src/blob.bin").binary);
        assert_eq!(view(&folder, "src/gone.txt").error.as_deref(), Some("error.file.missing"));
        assert_eq!(view(&folder, "src").error.as_deref(), Some("error.file.read"), "a folder is not a file");
        assert_eq!(view(&folder, "../x.txt").error.as_deref(), Some("error.file.outside"));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
