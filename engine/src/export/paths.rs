//! Replace only roots supplied by the caller; never guess whether ordinary text is a path.

pub struct Paths {
    roots: Vec<(String, String)>,
    windows: bool,
}

impl Paths {
    pub fn new(working_folder: &str, home: &str, windows: bool) -> Self {
        let mut roots: Vec<_> = [(working_folder, super::file_name(working_folder)), (home, "~")]
            .into_iter()
            .filter_map(|(root, label)| {
                let root = root.trim_end_matches(['/', '\\']);
                (!root.is_empty()).then(|| (root.to_string(), label.to_string()))
            })
            .collect();
        // A working folder inside the home folder keeps its own name, rather than becoming ~/... .
        roots.sort_by_key(|(root, _)| std::cmp::Reverse(root.len()));
        Self { roots, windows }
    }

    pub(super) fn redact(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while !rest.is_empty() {
            // Literal web addresses remain intact, including a known root in their URL path.
            if super::message::external_link(rest) {
                let end = rest.find(|ch: char| ch.is_whitespace() || matches!(ch, '\'' | '"' | '`' | '<' | '>')).unwrap_or(rest.len());
                out.push_str(&rest[..end]);
                rest = &rest[end..];
                continue;
            }
            let found = self.roots.iter().find_map(|(root, label)| {
                let mut chars = rest.char_indices();
                let mut end = 0;
                for expected in root.chars() {
                    let (at, actual) = chars.next()?;
                    let same = if matches!(expected, '/' | '\\') {
                        matches!(actual, '/' | '\\')
                    } else if self.windows {
                        actual.to_lowercase().eq(expected.to_lowercase())
                    } else {
                        actual == expected
                    };
                    if !same {
                        return None;
                    }
                    end = at + actual.len_utf8();
                }
                Some((end, label))
            });
            if let Some((end, label)) = found {
                out.push_str(label);
                rest = &rest[end..];
            } else {
                let ch = rest.chars().next().expect("nonempty suffix");
                out.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
        }
        out
    }
}
