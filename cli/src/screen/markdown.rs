//! Text for the terminal: the cleaning every text from the AI goes through, word wrap with styles, and light markdown
//! (headings, lists, quotes, code blocks, tables, bold, italic, code and links).

use super::palette::{Palette, Tone};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

/// One character with the style it is drawn in.
pub type Cell = (char, Style);

/// A text from the AI or from a file is data: control characters (an escape sequence among them) never reach the terminal.
pub fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\t' => out.push_str("    "),
            '\n' => out.push('\n'),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

pub fn cell_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

pub fn text_width(text: &str) -> usize {
    text.chars().map(cell_width).sum()
}

fn cells_width(cells: &[Cell]) -> usize {
    cells.iter().map(|(c, _)| cell_width(*c)).sum()
}

pub fn cells(text: &str, style: Style) -> Vec<Cell> {
    text.chars().map(|c| (c, style)).collect()
}

/// Cells back into spans, one per run of the same style.
pub fn spans(cells: &[Cell]) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut style = Style::new();
    for (c, s) in cells {
        if !run.is_empty() && *s != style {
            spans.push(Span::styled(std::mem::take(&mut run), style));
        }
        style = *s;
        run.push(*c);
    }
    if !run.is_empty() {
        spans.push(Span::styled(run, style));
    }
    spans
}

/// `text` cut to `width` columns, with an ellipsis when something was cut.
pub fn truncate(text: &str, width: usize) -> String {
    if text_width(text) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = cell_width(c);
        if used + w + 1 > width {
            break;
        }
        out.push(c);
        used += w;
    }
    if width > 0 {
        out.push('…');
    }
    out
}

pub fn truncate_cells(cells: &[Cell], width: usize) -> Vec<Cell> {
    if cells_width(cells) <= width {
        return cells.to_vec();
    }
    let mut out = Vec::new();
    let mut used = 0;
    for (c, s) in cells {
        let w = cell_width(*c);
        if used + w + 1 > width {
            break;
        }
        out.push((*c, *s));
        used += w;
    }
    if width > 0 {
        out.push(('…', cells.last().map_or(Style::new(), |(_, s)| *s)));
    }
    out
}

fn trim_end(row: &mut Vec<Cell>) {
    while row.last().is_some_and(|(c, _)| *c == ' ') {
        row.pop();
    }
}

/// Word wrap: rows no wider than `width`, broken at spaces; a word wider than a row is cut. Spaces at a break are dropped.
pub fn wrap(cells: &[Cell], width: usize) -> Vec<Vec<Cell>> {
    let width = width.max(1);
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut row: Vec<Cell> = Vec::new();
    let mut used = 0;
    let mut i = 0;
    while i < cells.len() {
        let space = cells[i].0 == ' ';
        let mut j = i;
        while j < cells.len() && (cells[j].0 == ' ') == space {
            j += 1;
        }
        let token = &cells[i..j];
        let w = cells_width(token);
        if space {
            if used > 0 && used + w <= width {
                row.extend_from_slice(token);
                used += w;
            } else if used > 0 {
                trim_end(&mut row);
                rows.push(std::mem::take(&mut row));
                used = 0;
            } else if rows.is_empty() {
                // Indentation the author wrote at the start of a paragraph stays.
                let keep = token.len().min(width.saturating_sub(1));
                row.extend_from_slice(&token[..keep]);
                used += keep;
            }
        } else if used + w <= width {
            row.extend_from_slice(token);
            used += w;
        } else if w <= width {
            trim_end(&mut row);
            rows.push(std::mem::take(&mut row));
            row.extend_from_slice(token);
            used = w;
        } else {
            if used > 0 {
                trim_end(&mut row);
                rows.push(std::mem::take(&mut row));
                used = 0;
            }
            for cell in token {
                let cw = cell_width(cell.0);
                if used + cw > width {
                    rows.push(std::mem::take(&mut row));
                    used = 0;
                }
                row.push(*cell);
                used += cw;
            }
        }
        i = j;
    }
    trim_end(&mut row);
    if !row.is_empty() || rows.is_empty() {
        rows.push(row);
    }
    rows
}

/// Rows of exactly the characters that fit, with no regard for words (code).
fn hard_wrap(cells: &[Cell], width: usize) -> Vec<Vec<Cell>> {
    let width = width.max(1);
    let mut rows = vec![Vec::new()];
    let mut used = 0;
    for cell in cells {
        let w = cell_width(cell.0);
        if used + w > width {
            rows.push(Vec::new());
            used = 0;
        }
        if let Some(row) = rows.last_mut() {
            row.push(*cell);
        }
        used += w;
    }
    rows
}

/// Plain text, wrapped, with `first` before its first row and `rest` before the others (a hanging indent).
pub fn wrap_text(text: &str, style: Style, width: usize, first: Span<'static>, rest: &str) -> Vec<Line<'static>> {
    let indent = text_width(&first.content).max(text_width(rest));
    let mut lines = Vec::new();
    for paragraph in clean(text).split('\n') {
        for row in wrap(&cells(paragraph, style), width.saturating_sub(indent)) {
            let lead = if lines.is_empty() { first.clone() } else { Span::raw(rest.to_string()) };
            let mut line = vec![lead];
            line.extend(spans(&row));
            lines.push(Line::from(line));
        }
    }
    lines
}

/// Markdown inside a line: `code`, **bold**, *italic*, _italic_ and [links](url).
pub fn inline(text: &str, base: Style, p: &Palette) -> Vec<Cell> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let find = |from: usize, marker: &[char]| -> Option<usize> {
        (from..chars.len().saturating_sub(marker.len() - 1)).find(|&k| chars[k..].starts_with(marker))
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if chars.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) => {
                out.push((chars[i + 1], base));
                i += 2;
                continue;
            }
            '`' => {
                if let Some(end) = find(i + 1, &['`']).filter(|&e| e > i + 1) {
                    let style = base.patch(p.tone(Tone::Code));
                    out.extend(chars[i + 1..end].iter().map(|c| (*c, style)));
                    i = end + 1;
                    continue;
                }
            }
            '*' if chars.get(i + 1) == Some(&'*') => {
                if let Some(end) = find(i + 2, &['*', '*']).filter(|&e| e > i + 2) {
                    let inner: String = chars[i + 2..end].iter().collect();
                    out.extend(inline(&inner, base.add_modifier(Modifier::BOLD), p));
                    i = end + 2;
                    continue;
                }
            }
            '*' | '_' => {
                let word_edge = |k: Option<usize>| k.and_then(|k| chars.get(k)).is_none_or(|n| !n.is_alphanumeric());
                let opens = chars.get(i + 1).is_some_and(|n| !n.is_whitespace() && *n != c);
                let boundary = c == '*' || word_edge(i.checked_sub(1));
                if opens
                    && boundary
                    && let Some(end) =
                        find(i + 1, &[c]).filter(|&e| e > i + 1 && !chars[e - 1].is_whitespace() && (c == '*' || word_edge(Some(e + 1))))
                {
                    let inner: String = chars[i + 1..end].iter().collect();
                    out.extend(inline(&inner, p.italic(base), p));
                    i = end + 1;
                    continue;
                }
            }
            '[' => {
                if let Some(close) = find(i + 1, &[']']).filter(|&e| e > i + 1 && chars.get(e + 1) == Some(&'('))
                    && let Some(end) = find(close + 2, &[')'])
                {
                    let label: String = chars[i + 1..close].iter().collect();
                    let url: String = chars[close + 2..end].iter().collect();
                    out.extend(inline(&label, base.add_modifier(Modifier::UNDERLINED), p));
                    out.extend(cells(&format!(" ({url})"), p.tone(Tone::Quiet)));
                    i = end + 1;
                    continue;
                }
            }
            _ => {}
        }
        out.push((c, base));
        i += 1;
    }
    out
}

fn line_of(prefix: Vec<Span<'static>>, row: &[Cell]) -> Line<'static> {
    let mut all = prefix;
    all.extend(spans(row));
    Line::from(all)
}

/// A list marker at the start of a (trimmed) line: its width in the source, what to draw for it, and the text after it.
fn list_marker(trimmed: &str) -> Option<(String, &str)> {
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(bullet) {
            return Some(("• ".to_string(), rest));
        }
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        let after = &trimmed[digits..];
        for dot in [". ", ") "] {
            if let Some(rest) = after.strip_prefix(dot) {
                return Some((format!("{}{dot}", &trimmed[..digits]), rest));
            }
        }
    }
    None
}

fn is_rule(trimmed: &str) -> bool {
    let mut chars = trimmed.chars().filter(|c| !c.is_whitespace());
    chars.next().is_some_and(|first| {
        matches!(first, '-' | '*' | '_') && trimmed.chars().filter(|c| !c.is_whitespace()).count() >= 3 && chars.all(|c| c == first)
    })
}

fn table_cells(row: &str) -> Vec<&str> {
    let row = row.trim();
    let row = row.strip_prefix('|').unwrap_or(row);
    let row = row.strip_suffix('|').unwrap_or(row);
    row.split('|').map(str::trim).collect()
}

fn is_separator(row: &str) -> bool {
    let cells = table_cells(row);
    !cells.is_empty() && cells.iter().all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':')) && c.contains('-'))
}

/// A table cut to the width: the columns shrink, widest first, and a cell that does not fit ends in an ellipsis.
fn table(rows: &[&str], width: usize, p: &Palette) -> Vec<Line<'static>> {
    let mut body: Vec<Vec<Vec<Cell>>> = Vec::new();
    let mut header_rows = 0;
    for (n, row) in rows.iter().enumerate() {
        if n == 1 && is_separator(row) {
            header_rows = 1;
            continue;
        }
        let style = if n == 0 && rows.get(1).is_some_and(|r| is_separator(r)) { p.tone(Tone::Strong) } else { p.tone(Tone::Text) };
        body.push(table_cells(row).into_iter().map(|c| inline(c, style, p)).collect());
    }
    let columns = body.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 {
        return Vec::new();
    }
    let mut widths: Vec<usize> =
        (0..columns).map(|c| body.iter().filter_map(|r| r.get(c)).map(|cell| cells_width(cell)).max().unwrap_or(0).max(1)).collect();
    let gaps = 3 * (columns - 1);
    while widths.iter().sum::<usize>() + gaps > width {
        let Some((widest, w)) = widths.iter().copied().enumerate().max_by_key(|(_, w)| *w) else { break };
        if w <= 3 {
            break;
        }
        widths[widest] -= 1;
    }
    let quiet = p.tone(Tone::Quiet);
    let mut lines = Vec::new();
    for (n, row) in body.iter().enumerate() {
        let mut spans_row: Vec<Span<'static>> = Vec::new();
        for (c, w) in widths.iter().enumerate() {
            if c > 0 {
                spans_row.push(Span::styled(" │ ", quiet));
            }
            let cell = truncate_cells(row.get(c).map_or(&[][..], Vec::as_slice), *w);
            let pad = w.saturating_sub(cells_width(&cell));
            spans_row.extend(spans(&cell));
            spans_row.push(Span::raw(" ".repeat(pad)));
        }
        lines.push(clip(Line::from(spans_row), width));
        if n == 0 && header_rows == 1 {
            let rule: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
            lines.push(Line::from(Span::styled(rule.join("─┼─"), quiet)));
        }
    }
    lines
}

/// A line cut to `width` columns (a table with more columns than fit).
fn clip(line: Line<'static>, width: usize) -> Line<'static> {
    let all: Vec<Cell> = line.spans.iter().flat_map(|s| s.content.chars().map(|c| (c, s.style))).collect();
    if cells_width(&all) <= width {
        return line;
    }
    Line::from(spans(&truncate_cells(&all, width)))
}

/// Markdown as lines no wider than `width`.
pub fn render(text: &str, width: usize, p: &Palette) -> Vec<Line<'static>> {
    let text = clean(text);
    let width = width.max(8);
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut lines = text.lines().peekable();
    let mut fence: Option<String> = None;
    let blank = |out: &mut Vec<Line<'static>>| {
        if out.last().is_some_and(|l| !l.spans.is_empty()) {
            out.push(Line::default());
        }
    };
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if let Some(open) = &fence {
            if trimmed.starts_with(open.as_str()) {
                fence = None;
                continue;
            }
            let style = p.tone(Tone::Code);
            for row in hard_wrap(&cells(&format!("  {line}"), style), width) {
                out.push(Line::from(spans(&row)).style(p.code_ground()));
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = Some(trimmed.chars().take(3).collect());
            continue;
        }
        if trimmed.is_empty() {
            blank(&mut out);
            continue;
        }
        if trimmed.starts_with('|') {
            let mut rows = vec![trimmed];
            while let Some(next) = lines.next_if(|l| l.trim_start().starts_with('|')) {
                rows.push(next.trim_start());
            }
            out.extend(table(&rows, width, p));
            continue;
        }
        if is_rule(trimmed) {
            out.push(Line::from(Span::styled("─".repeat(width.min(40)), p.rule())));
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            blank(&mut out);
            let style = p.tone(Tone::Heading);
            for row in wrap(&inline(trimmed[hashes..].trim(), style, p), width) {
                out.push(line_of(Vec::new(), &row));
            }
            continue;
        }
        if let Some(quote) = trimmed.strip_prefix('>') {
            let quiet = p.tone(Tone::Quiet);
            for row in wrap(&inline(quote.trim_start(), quiet, p), width.saturating_sub(2)) {
                out.push(line_of(vec![Span::styled("│ ", quiet)], &row));
            }
            continue;
        }
        let indent = line.len() - trimmed.len();
        let (lead, body) = match list_marker(trimmed) {
            Some((marker, rest)) => (format!("{}{marker}", " ".repeat(indent.min(width / 2))), rest),
            None => (String::new(), trimmed),
        };
        let keep = if lead.is_empty() { line } else { body };
        let hang = " ".repeat(text_width(&lead));
        let rows = wrap(&inline(keep, p.tone(Tone::Text), p), width.saturating_sub(text_width(&lead)));
        for (n, row) in rows.iter().enumerate() {
            let prefix = if n == 0 && !lead.is_empty() { Span::styled(lead.clone(), p.tone(Tone::Quiet)) } else { Span::raw(hang.clone()) };
            out.push(line_of(if prefix.content.is_empty() { Vec::new() } else { vec![prefix] }, row));
        }
    }
    while out.last().is_some_and(|l| l.spans.is_empty()) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Palette {
        Palette::plain()
    }

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>()).collect()
    }

    #[test]
    fn control_characters_never_reach_the_terminal() {
        assert_eq!(clean("a\u{1b}[31mb\u{7}\r\n\tc"), "a[31mb\n    c");
    }

    #[test]
    fn wrap_breaks_at_spaces_and_cuts_long_words() {
        let rows: Vec<String> =
            wrap(&cells("one two three  four", Style::new()), 8).iter().map(|r| r.iter().map(|(c, _)| *c).collect()).collect();
        assert_eq!(rows, ["one two", "three", "four"]);
        let rows: Vec<String> =
            wrap(&cells("abcdefghij kl", Style::new()), 4).iter().map(|r| r.iter().map(|(c, _)| *c).collect()).collect();
        assert_eq!(rows, ["abcd", "efgh", "ij", "kl"]);
        assert_eq!(wrap(&[], 10).len(), 1);
    }

    #[test]
    fn wide_characters_count_two_columns() {
        let rows = wrap(&cells("日本語日本語", Style::new()), 6);
        assert_eq!(rows.len(), 2);
        assert_eq!(truncate("日本語日本語", 7), "日本語…");
    }

    #[test]
    fn headings_lists_quotes_and_code_are_drawn() {
        let md = "# Title\n\nSome **bold** and `code`.\n\n- first item that is long enough to wrap around\n1. numbered\n\n> quoted\n\n```rust\nfn main() {}\n```\n";
        let lines = text(&render(md, 24, &plain()));
        assert_eq!(lines[0], "Title");
        assert!(lines.contains(&"Some bold and code.".to_string()), "{lines:?}");
        assert!(lines.contains(&"• first item that is".to_string()), "{lines:?}");
        assert!(lines.contains(&"  long enough to wrap".to_string()), "{lines:?}");
        assert!(lines.contains(&"1. numbered".to_string()));
        assert!(lines.contains(&"│ quoted".to_string()));
        assert!(lines.contains(&"  fn main() {}".to_string()));
        assert!(lines.iter().all(|l| text_width(l) <= 24));
    }

    #[test]
    fn bold_and_code_carry_their_style_and_snake_case_stays() {
        let line = &render("a **b** c_d_e", 40, &Palette::plain())[0];
        assert!(line.spans.iter().any(|s| s.content == "b" && s.style.add_modifier.contains(Modifier::BOLD)));
        assert_eq!(text(&render("a_b_c and 2*3*4", 40, &plain()))[0], "a_b_c and 2*3*4".replace("2*3*4", "234"));
    }

    #[test]
    fn links_show_their_address_and_tables_fit_the_width() {
        assert_eq!(text(&render("see [the docs](https://x.dev) now", 60, &plain()))[0], "see the docs (https://x.dev) now");
        let table = "| Name | Value |\n| --- | --- |\n| alpha | 1 |\n| a much longer cell than the rest | 2 |";
        let lines = text(&render(table, 24, &plain()));
        assert_eq!(lines.len(), 4);
        assert!(lines.iter().all(|l| text_width(l) <= 24), "{lines:?}");
        assert!(lines[1].contains('┼'));
        assert!(lines[3].contains('…'));
    }

    #[test]
    fn an_unclosed_fence_or_marker_does_not_swallow_the_rest() {
        let lines = text(&render("a ** b\n```\ncode", 30, &plain()));
        assert_eq!(lines, ["a ** b", "  code"]);
    }
}
