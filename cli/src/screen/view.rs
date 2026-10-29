//! Drawing the screen: the list on the left, the open conversation in the middle, the card, the box and the status bar.

use super::app::{Account, App, Focus, Geometry, Mode, VENDORS};
use super::markdown::{cell_width, spans, text_width, truncate, truncate_cells};
use super::overlay;
use super::palette::{Colors, Tone};
use crate::thread::Thread;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

/// The least the screen can be drawn in.
const MIN_WIDTH: u16 = 40;
const MIN_HEIGHT: u16 = 8;
/// Below this width an open list takes the whole screen.
const LIST_ONLY: u16 = 70;
const LIST_WIDTH: u16 = 28;
const INPUT_ROWS: usize = 5;
const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const FRAMES_PLAIN: [&str; 4] = ["|", "/", "-", "\\"];

/// The working mark: it turns while an answer streams, and stands still when the person asked for less motion.
pub fn mark(app: &App) -> &'static str {
    let plain = app.palette.colors == Colors::Plain;
    if app.reduced_motion {
        return if plain { "*" } else { "●" };
    }
    if plain { FRAMES_PLAIN[app.frame % FRAMES_PLAIN.len()] } else { FRAMES[app.frame % FRAMES.len()] }
}

fn put(f: &mut Frame, rect: Rect, lines: Vec<Line<'static>>) {
    if rect.width > 0 && rect.height > 0 {
        f.render_widget(Paragraph::new(lines), rect);
    }
}

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.size = (area.width, area.height);
    f.buffer_mut().set_style(area, app.palette.ground());
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let text = truncate(&app.t("cli.screen.tooSmall"), usize::from(area.width));
        put(f, area, vec![Line::from(Span::styled(text, app.palette.tone(Tone::Warn)))]);
        app.geo = Geometry::default();
        return;
    }
    let body_h = area.height - 1;
    let status = Rect::new(0, body_h, area.width, 1);
    let side_w = if app.show_sidebar() { if area.width < LIST_ONLY { area.width } else { LIST_WIDTH } } else { 0 };
    if side_w > 0 {
        list(f, app, Rect::new(0, 0, side_w, body_h));
    } else {
        app.geo.list = None;
    }
    if side_w < area.width {
        main(f, app, Rect::new(side_w, 0, area.width - side_w, body_h));
    }
    status_bar(f, app, status);
    overlay::popup(f, app, area);
}

// ---- the list ----

fn list(f: &mut Frame, app: &mut App, rect: Rect) {
    let p = app.palette.clone();
    f.buffer_mut().set_style(rect, p.side());
    let wide = usize::from(rect.width).saturating_sub(2);
    put(
        f,
        Rect::new(rect.x, rect.y, rect.width.saturating_sub(1), 1),
        vec![Line::from(Span::styled(format!(" {}", truncate(&app.t("cli.screen.list.title"), wide)), p.tone(Tone::Strong)))],
    );
    if rect.width < LIST_ONLY {
        let rule: Vec<Line<'static>> = (0..rect.height).map(|_| Line::from(Span::styled("│", p.rule()))).collect();
        put(f, Rect::new(rect.x + rect.width - 1, rect.y, 1, rect.height), rule);
    }
    let rows = Rect::new(rect.x, rect.y + 1, rect.width.saturating_sub(1), rect.height.saturating_sub(1));
    app.geo.list = Some(rows);
    let visible = usize::from(rows.height);
    if visible == 0 {
        return;
    }
    if app.list_cursor < app.list_top {
        app.list_top = app.list_cursor;
    } else if app.list_cursor >= app.list_top + visible {
        app.list_top = app.list_cursor + 1 - visible;
    }
    let width = usize::from(rows.width);
    let mut lines = Vec::new();
    for row in app.list_top..(app.list_top + visible).min(app.list_len()) {
        let (mark_text, title, tone, active) = if row == 0 {
            ("+".to_string(), app.t("cli.list.untitled"), Tone::Accent, app.active.is_none())
        } else {
            let conv = &app.conversations[row - 1];
            let mark_text = if app.needs_you(&conv.id) {
                "!".to_string()
            } else if app.is_running(&conv.id) {
                self::mark(app).to_string()
            } else if conv.pinned {
                "★".to_string()
            } else {
                " ".to_string()
            };
            let title =
                if conv.title.is_empty() { app.t("cli.list.untitled") } else { super::markdown::clean(&conv.title).replace('\n', " ") };
            let tone = if app.needs_you(&conv.id) { Tone::Warn } else { Tone::Text };
            (mark_text, title, tone, app.active.as_deref() == Some(&conv.id))
        };
        let mut style = p.tone(tone);
        if active {
            style = style.add_modifier(Modifier::BOLD);
        }
        let mut line = Line::from(vec![
            Span::styled(
                format!(" {mark_text} "),
                if row > 0 && app.needs_you(&app.conversations[row - 1].id) { p.tone(Tone::Warn) } else { p.tone(Tone::Accent) },
            ),
            Span::styled(truncate(&title, width.saturating_sub(4)), style),
        ]);
        if row == app.list_cursor && app.focus == Focus::List {
            line = line.style(p.selected());
        }
        lines.push(line);
    }
    if app.conversations.is_empty() && visible > 2 {
        lines.push(Line::default());
        let empty = [app.t("cli.screen.list.empty"), app.t("cli.screen.list.emptyHint")];
        for text in empty {
            lines.extend(super::markdown::wrap_text(&text, p.tone(Tone::Quiet), width, Span::raw(" "), " "));
        }
    }
    put(f, rows, lines);
}

// ---- the middle ----

fn main(f: &mut Frame, app: &mut App, rect: Rect) {
    let p = app.palette.clone();
    let width = usize::from(rect.width);
    // The box, from the width it has.
    let layout = app.editor.layout(width.saturating_sub(2));
    let input_h = u16::try_from(layout.rows.len().clamp(1, INPUT_ROWS)).unwrap_or(1);
    let card = app.card.as_ref().map(|c| {
        let full = overlay::card_lines(app, c, width, true);
        let room = usize::from(rect.height).saturating_sub(usize::from(input_h) + 5).max(3);
        if full.len() <= room { full } else { overlay::card_lines(app, c, width, false) }
    });
    let banner = u16::from(app.card_deferred());
    let fixed = 1 + 1 + input_h + banner;
    let room = rect.height.saturating_sub(fixed + 2);
    let card_h = card.as_ref().map_or(0, |c| u16::try_from(c.len()).unwrap_or(u16::MAX).min(room));
    let thread_h = rect.height.saturating_sub(fixed + card_h);
    let title = Rect::new(rect.x, rect.y, rect.width, 1);
    let thread_rect = Rect::new(rect.x, rect.y + 1, rect.width, thread_h);
    let banner_rect = Rect::new(rect.x, thread_rect.y + thread_h, rect.width, banner);
    let card_rect = Rect::new(rect.x, banner_rect.y + banner, rect.width, card_h);
    let rule = Rect::new(rect.x, card_rect.y + card_h, rect.width, 1);
    let input = Rect::new(rect.x, rule.y + 1, rect.width, input_h);
    app.geo.thread = thread_rect;
    app.geo.input = input;

    // Title: where the conversation is, and whether it is working.
    let (area_name, name) = app.crumb();
    let working = app.active.as_deref().is_some_and(|id| app.is_running(id));
    let tail = if working { format!("{} {} ", mark(app), app.t("cli.screen.working")) } else { String::new() };
    let head = truncate(&format!(" {area_name} › {name}"), width.saturating_sub(text_width(&tail) + 1));
    let gap = width.saturating_sub(text_width(&head) + text_width(&tail));
    put(
        f,
        title,
        vec![Line::from(vec![
            Span::styled(head, p.tone(Tone::Strong)),
            Span::raw(" ".repeat(gap)),
            Span::styled(tail, p.tone(Tone::Accent)),
        ])],
    );

    thread_area(f, app, thread_rect, working);

    if banner == 1 {
        put(
            f,
            banner_rect,
            vec![Line::from(Span::styled(
                format!(" {}", truncate(&app.t("cli.screen.card.waiting"), width.saturating_sub(2))),
                p.tone(Tone::Warn),
            ))],
        );
    }
    if let Some(lines) = card {
        f.buffer_mut().set_style(card_rect, p.raised());
        put(f, card_rect, lines);
    }

    // The rule, with the keys that matter now at its right.
    let hint = app.t(match (app.mode, app.focus) {
        (Mode::Normal, Focus::List) => "cli.screen.hint.list",
        (Mode::Normal, Focus::Thread) => "cli.screen.hint.thread",
        _ => "cli.screen.hint.box",
    });
    let hint = truncate(&hint, width.saturating_sub(6));
    let rule_len = width.saturating_sub(text_width(&hint) + 3);
    put(
        f,
        rule,
        vec![Line::from(vec![Span::styled("─".repeat(rule_len), p.rule()), Span::styled(format!(" {hint} "), p.tone(Tone::Quiet))])],
    );

    // The box.
    f.buffer_mut().set_style(input, p.raised());
    let first = (layout.cursor_row + 1).saturating_sub(usize::from(input_h));
    let mut lines = Vec::new();
    if app.editor.is_empty() {
        let key = match app.mode {
            Mode::Normal => "cli.screen.box.placeholder",
            Mode::Rename => "cli.screen.box.rename",
            Mode::Answer => "cli.screen.box.answer",
        };
        lines.push(Line::from(vec![
            Span::styled("› ", p.tone(Tone::Accent).add_modifier(Modifier::BOLD)),
            Span::styled(truncate(&app.t(key), width.saturating_sub(3)), p.tone(Tone::Quiet)),
        ]));
    } else {
        for (n, row) in layout.rows.iter().enumerate().skip(first).take(usize::from(input_h)) {
            let lead = if n == 0 { Span::styled("› ", p.tone(Tone::Accent).add_modifier(Modifier::BOLD)) } else { Span::raw("  ") };
            lines.push(Line::from(vec![lead, Span::styled(row.clone(), p.tone(Tone::Text))]));
        }
    }
    put(f, input, lines);
    if app.commands_open() {
        overlay::commands(f, app, rect, rule.y);
    }
    if app.focus == Focus::Box && app.popup.is_none() {
        let col = u16::try_from(layout.cursor_col).unwrap_or(0);
        let row = u16::try_from(layout.cursor_row.saturating_sub(first)).unwrap_or(0);
        f.set_cursor_position(Position::new(
            (input.x + 2 + col).min(input.x + input.width.saturating_sub(1)),
            input.y + row.min(input_h - 1),
        ));
    }
}

fn thread_area(f: &mut Frame, app: &mut App, rect: Rect, working: bool) {
    if rect.height == 0 {
        return;
    }
    let p = app.palette.clone();
    let inner = Rect::new(rect.x + 1, rect.y, rect.width.saturating_sub(2), rect.height);
    let width = usize::from(inner.width);
    let empty = app.thread.as_ref().is_none_or(|t: &Thread| t.order.is_empty());
    if empty {
        app.transcript.sync(None, width, &p, &app.words);
        app.last_total = 0;
        let lines = empty_state(app, width);
        put(f, inner, lines);
        return;
    }
    app.transcript.sync(app.thread.as_ref(), width, &p, &app.words);
    let extra = usize::from(working);
    let total = app.transcript.total() + extra;
    let height = usize::from(inner.height);
    // A person reading further up keeps their place while the answer grows below.
    if app.scroll > 0 && total > app.last_total {
        app.scroll += total - app.last_total;
    }
    app.last_total = total;
    app.scroll = app.scroll.min(total.saturating_sub(height));
    let end = total - app.scroll;
    let start = end.saturating_sub(height);
    let mut lines = app.transcript.window(start, end - start);
    if working && app.scroll == 0 {
        lines.push(Line::from(Span::styled(format!("{} {}", mark(app), app.t("cli.screen.working")), p.tone(Tone::Accent))));
    }
    put(f, inner, lines);
    if app.scroll > 0 {
        let hint = format!(" {} ", truncate(&app.t("cli.screen.latest"), width.saturating_sub(2)));
        let w = u16::try_from(text_width(&hint)).unwrap_or(0).min(inner.width);
        let at = Rect::new(inner.x + inner.width - w, inner.y + inner.height - 1, w, 1);
        f.render_widget(Clear, at);
        f.buffer_mut().set_style(at, p.raised());
        put(f, at, vec![Line::from(Span::styled(hint, p.tone(Tone::Accent)))]);
    }
}

/// What the thread says before there is a thread: nothing connected, nothing yet, or a conversation opening.
fn empty_state(app: &App, width: usize) -> Vec<Line<'static>> {
    let p = &app.palette;
    let quiet = p.tone(Tone::Quiet);
    if app.loading {
        return vec![Line::default(), Line::from(Span::styled(app.t("cli.screen.opening"), quiet))];
    }
    if app.active.is_some() {
        return Vec::new();
    }
    let known = app.accounts.iter().all(|a| *a != Account::Unknown);
    let none = known && app.accounts.iter().all(|a| *a != Account::SignedIn);
    let mut lines = vec![Line::default()];
    if none {
        lines.extend(super::markdown::wrap_text(&app.t("cli.screen.noAi.title"), p.tone(Tone::Strong), width, Span::raw(""), ""));
        lines.push(Line::default());
        lines.extend(super::markdown::wrap_text(&app.t("cli.screen.noAi.hint"), p.tone(Tone::Text), width, Span::raw(""), ""));
        for vendor in VENDORS {
            lines.push(Line::from(Span::styled(format!("  faber sign-in {vendor}"), p.tone(Tone::Accent))));
        }
    } else {
        lines.extend(super::markdown::wrap_text(&app.t("cli.screen.thread.empty"), quiet, width, Span::raw(""), ""));
    }
    lines
}

// ---- the status bar ----

fn pct_tone(used: f64) -> Tone {
    if used >= 0.9 {
        Tone::Bad
    } else if used >= 0.7 {
        Tone::Warn
    } else {
        Tone::Quiet
    }
}

/// The vendor whose quota the bar shows: the one that answered last, else the first signed in.
fn active_vendor(app: &App) -> &'static str {
    let answered = app
        .thread
        .as_ref()
        .and_then(|t| t.order.iter().rev().filter_map(|id| t.get(id)).find_map(|m| m["vendor"].as_str().map(str::to_string)));
    if let Some(v) = answered.and_then(|v| VENDORS.iter().copied().find(|k| *k == v)) {
        return v;
    }
    VENDORS.iter().zip(app.accounts).find(|(_, a)| *a == Account::SignedIn).map_or(VENDORS[0], |(v, _)| *v)
}

fn status_spans(app: &App, windows: usize) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let p = &app.palette;
    let mut left: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (vendor, account) in VENDORS.iter().zip(app.accounts) {
        let name = Span::styled(app.t(&format!("cli.vendor.{vendor}")), p.vendor(vendor));
        let rest = match account {
            Account::SignedIn => Span::styled(" ✓", p.tone(Tone::Good)),
            Account::SignedOut => Span::styled(format!(": {}", app.t("cli.account.signedOut")), p.tone(Tone::Warn)),
            Account::Missing => Span::styled(format!(": {}", app.t("cli.account.notInstalled")), p.tone(Tone::Warn)),
            Account::Unknown => continue,
        };
        if left.len() > 1 {
            left.push(Span::raw("  "));
        }
        left.extend([name, rest]);
    }
    let vendor = active_vendor(app);
    for quota in app.quotas.iter().filter(|q| q.vendor == vendor).take(windows) {
        let used = quota.used.clamp(0.0, 1.0);
        left.push(Span::styled(" · ", p.tone(Tone::Quiet)));
        left.push(Span::styled(format!("{} {}%", app.window_name(&quota.window), (used * 100.0).round()), p.tone(pct_tone(used))));
    }
    let mut right: Vec<Span<'static>> = Vec::new();
    if !app.running.is_empty() {
        right.push(Span::styled(app.words.count("cli.running", app.running.len()), p.tone(Tone::Text)));
        let needs = app.running.iter().filter(|r| r.needs_you).count();
        if needs > 0 {
            right.push(Span::styled(" · ", p.tone(Tone::Quiet)));
            right.push(Span::styled(app.words.count("cli.needsYou", needs), p.tone(Tone::Warn)));
        }
        right.push(Span::raw(" "));
    }
    (left, right)
}

fn width_of(spans: &[Span<'static>]) -> usize {
    spans.iter().map(|s| text_width(&s.content)).sum()
}

fn status_bar(f: &mut Frame, app: &App, rect: Rect) {
    let p = &app.palette;
    f.buffer_mut().set_style(rect, p.raised());
    let width = usize::from(rect.width);
    let toast = app.toast.clone().or_else(|| app.leave_hint_live().then(|| app.t("cli.screen.leave")));
    let (mut left, right) = status_spans(app, 2);
    if let Some(text) = toast {
        left = vec![Span::styled(format!(" {}", truncate(&markdown_clean(&text), width.saturating_sub(2))), p.tone(Tone::Warn))];
    } else {
        // Drop the quota windows one by one, then cut, until the bar fits.
        for windows in [2, 1, 0] {
            left = status_spans(app, windows).0;
            if width_of(&left) + width_of(&right) < width {
                break;
            }
        }
    }
    let room = width.saturating_sub(width_of(&right));
    let cells: Vec<_> = left.iter().flat_map(|s| s.content.chars().map(|c| (c, s.style))).collect();
    let cells = truncate_cells(&cells, room);
    let used: usize = cells.iter().map(|(c, _)| cell_width(*c)).sum();
    let mut line = spans(&cells);
    line.push(Span::raw(" ".repeat(room.saturating_sub(used))));
    line.extend(right);
    put(f, rect, vec![Line::from(line)]);
}

fn markdown_clean(text: &str) -> String {
    super::markdown::clean(text).replace('\n', " ")
}
