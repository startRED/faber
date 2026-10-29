//! The box: a few lines of text with a cursor. The cursor is a character index; the box draws its text cut to rows of the
//! width it has, so a row and a column are always a character and a number of cells.

use super::markdown::cell_width;

#[derive(Default, Clone, Debug)]
pub struct Editor {
    text: String,
    /// In characters, from the start of `text`.
    cursor: usize,
}

/// The text cut into the rows the box shows, and where the cursor is among them.
#[derive(Debug, PartialEq)]
pub struct Layout {
    pub rows: Vec<String>,
    /// The character index each row starts at.
    starts: Vec<usize>,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

impl Editor {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// Replaces the text, with the cursor at its end (a text brought back from the history).
    pub fn set(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.len();
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, chars: usize) -> usize {
        self.text.char_indices().nth(chars).map_or(self.text.len(), |(b, _)| b)
    }

    /// Typed or pasted text: new lines stay, other control characters go.
    pub fn insert(&mut self, text: &str) {
        let clean: String = text
            .replace("\r\n", "\n")
            .chars()
            .filter_map(|c| match c {
                '\r' => Some('\n'),
                '\t' => Some(' '),
                '\n' => Some('\n'),
                c if c.is_control() => None,
                c => Some(c),
            })
            .collect();
        let at = self.byte(self.cursor);
        self.text.insert_str(at, &clean);
        self.cursor += clean.chars().count();
    }

    pub fn newline(&mut self) {
        self.insert("\n");
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let (from, to) = (self.byte(self.cursor - 1), self.byte(self.cursor));
            self.text.replace_range(from..to, "");
            self.cursor -= 1;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.len() {
            let (from, to) = (self.byte(self.cursor), self.byte(self.cursor + 1));
            self.text.replace_range(from..to, "");
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.len());
    }

    fn chars(&self) -> Vec<char> {
        self.text.chars().collect()
    }

    /// The start of the line (a line of the text, not a row of the box) the cursor is in.
    pub fn home(&mut self) {
        let chars = self.chars();
        while self.cursor > 0 && chars[self.cursor - 1] != '\n' {
            self.cursor -= 1;
        }
    }

    pub fn end(&mut self) {
        let chars = self.chars();
        while self.cursor < chars.len() && chars[self.cursor] != '\n' {
            self.cursor += 1;
        }
    }

    pub fn word_left(&mut self) {
        let chars = self.chars();
        while self.cursor > 0 && !chars[self.cursor - 1].is_alphanumeric() {
            self.cursor -= 1;
        }
        while self.cursor > 0 && chars[self.cursor - 1].is_alphanumeric() {
            self.cursor -= 1;
        }
    }

    pub fn word_right(&mut self) {
        let chars = self.chars();
        while self.cursor < chars.len() && !chars[self.cursor].is_alphanumeric() {
            self.cursor += 1;
        }
        while self.cursor < chars.len() && chars[self.cursor].is_alphanumeric() {
            self.cursor += 1;
        }
    }

    pub fn delete_word_back(&mut self) {
        let end = self.cursor;
        self.word_left();
        let (from, to) = (self.byte(self.cursor), self.byte(end));
        self.text.replace_range(from..to, "");
    }

    pub fn delete_to_line_start(&mut self) {
        let end = self.cursor;
        self.home();
        let (from, to) = (self.byte(self.cursor), self.byte(end));
        self.text.replace_range(from..to, "");
    }

    /// The text in rows of at most `width` cells, and the cursor's row and column.
    pub fn layout(&self, width: usize) -> Layout {
        let width = width.max(2);
        let mut rows = Vec::new();
        let mut starts = Vec::new();
        let mut at = 0;
        let (mut cursor_row, mut cursor_col) = (0, 0);
        for line in self.text.split('\n') {
            let chars: Vec<char> = line.chars().collect();
            let mut start = 0;
            loop {
                let (mut used, mut end) = (0, start);
                while end < chars.len() && used + cell_width(chars[end]) <= width {
                    used += cell_width(chars[end]);
                    end += 1;
                }
                let last = end >= chars.len();
                if (at + start..=at + end).contains(&self.cursor) && (self.cursor < at + end || last) {
                    cursor_row = rows.len();
                    cursor_col = chars[start..self.cursor - at].iter().map(|c| cell_width(*c)).sum();
                }
                rows.push(chars[start..end].iter().collect::<String>());
                starts.push(at + start);
                if last {
                    break;
                }
                start = end;
            }
            at += chars.len() + 1;
        }
        // The cursor after a full last row of a line sits at the start of a row of its own.
        if cursor_col >= width {
            rows.insert(cursor_row + 1, String::new());
            starts.insert(cursor_row + 1, self.cursor);
            cursor_row += 1;
            cursor_col = 0;
        }
        Layout { rows, starts, cursor_row, cursor_col }
    }

    /// The cursor one row up; false when it is on the first row already.
    pub fn up(&mut self, width: usize) -> bool {
        self.vertical(width, false)
    }

    pub fn down(&mut self, width: usize) -> bool {
        self.vertical(width, true)
    }

    fn vertical(&mut self, width: usize, down: bool) -> bool {
        let layout = self.layout(width);
        let target = if down { layout.cursor_row + 1 } else { layout.cursor_row.wrapping_sub(1) };
        let Some(row) = layout.rows.get(target) else { return false };
        let mut used = 0;
        let mut chars = 0;
        for c in row.chars() {
            if used + cell_width(c) > layout.cursor_col {
                break;
            }
            used += cell_width(c);
            chars += 1;
        }
        self.cursor = layout.starts[target] + chars;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(text: &str) -> Editor {
        let mut e = Editor::default();
        e.insert(text);
        e
    }

    #[test]
    fn typing_and_deleting_work_on_characters_not_bytes() {
        let mut e = editor("olá");
        e.left();
        e.backspace();
        assert_eq!(e.text(), "oá");
        e.delete();
        assert_eq!(e.text(), "o");
        e.insert("ção");
        assert_eq!(e.text(), "oção");
    }

    #[test]
    fn pasted_text_keeps_its_lines_and_loses_control_characters() {
        let e = editor("a\r\nb\u{1b}[0m\tc");
        assert_eq!(e.text(), "a\nb[0m c");
    }

    #[test]
    fn words_and_line_ends_move_and_delete_within_the_text() {
        let mut e = editor("one two three");
        e.delete_word_back();
        assert_eq!(e.text(), "one two ");
        e.delete_word_back();
        assert_eq!(e.text(), "one ");
        let mut e = editor("ab\ncd");
        e.delete_to_line_start();
        assert_eq!(e.text(), "ab\n");
        e.left();
        e.home();
        e.insert(">");
        e.end();
        e.insert("<");
        assert_eq!(e.text(), ">ab<\n");
    }

    #[test]
    fn rows_follow_the_width_and_the_cursor_with_them() {
        let e = editor("abcdef gh");
        let layout = e.layout(4);
        assert_eq!(layout.rows, ["abcd", "ef g", "h"]);
        assert_eq!((layout.cursor_row, layout.cursor_col), (2, 1));
        let full = editor("abcd").layout(4);
        assert_eq!((full.rows.len(), full.cursor_row, full.cursor_col), (2, 1, 0));
        assert_eq!(editor("").layout(10).rows, [""]);
    }

    #[test]
    fn up_and_down_keep_the_column_and_stop_at_the_ends() {
        let mut e = editor("abc\nde\nfghij");
        assert!(e.up(20));
        assert!(e.up(20));
        assert!(!e.up(20));
        e.insert("X");
        assert_eq!(e.text(), "abXc\nde\nfghij");
        assert!(e.down(20));
        e.insert("Y");
        assert_eq!(e.text(), "abXc\ndeY\nfghij");
        assert!(e.down(20));
        assert!(!e.down(20));
    }
}
