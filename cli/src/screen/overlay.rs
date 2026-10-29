//! The parts that sit over the thread or above the box: the card, the list of commands and the help or quota popup.

use super::app::App;
use super::card::{Card, Kind};
use super::markdown::{self, text_width, truncate, wrap_text};
use super::palette::Tone;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

/// Plan lines shown on a card; the rest is in the window.
const PLAN_LINES: usize = 8;

/// The card as lines no wider than `width`, with or without the description of each option.
pub fn card_lines(app: &App, card: &Card, width: usize, about: bool) -> Vec<Line<'static>> {
    let p = &app.palette;
    let bar = Span::styled("▎ ", p.tone(Tone::Warn));
    let inner = width.saturating_sub(2).max(10);
    let mut body: Vec<Line<'static>> = Vec::new();
    let option = |body: &mut Vec<Line<'static>>, n: usize, label: &str, mark: &str, details: &str| {
        let here = card.cursor == n;
        let lead = format!("{}{} ", if here { "›" } else { " " }, n + 1);
        let style = if here { p.tone(Tone::Strong) } else { p.tone(Tone::Text) };
        let mut line = vec![Span::styled(lead, p.tone(Tone::Accent).add_modifier(Modifier::BOLD))];
        if !mark.is_empty() {
            line.push(Span::styled(format!("{mark} "), p.tone(Tone::Quiet)));
        }
        line.push(Span::styled(truncate(&markdown::clean(label), inner.saturating_sub(4 + mark.len())), style));
        body.push(Line::from(line));
        if about && !details.is_empty() {
            body.extend(wrap_text(details, p.tone(Tone::Quiet), inner, Span::raw("    "), "    "));
        }
    };
    match &card.kind {
        Kind::Questions { .. } => {
            if let Some((question, at, total)) = card.question() {
                if total > 1 {
                    body.push(Line::from(Span::styled(
                        app.words.fill("cli.screen.card.progress", &[("n", &(at + 1).to_string()), ("total", &total.to_string())]),
                        p.tone(Tone::Quiet),
                    )));
                }
                body.extend(wrap_text(&question.text, p.tone(Tone::Strong), inner, Span::raw(""), ""));
                if !card.own {
                    for (n, choice) in question.options.iter().enumerate() {
                        let mark = if question.multi { if card.marked(n) { "[x]" } else { "[ ]" } } else { "" };
                        option(&mut body, n, &choice.label, mark, &choice.about);
                    }
                    if !question.options.is_empty() {
                        option(&mut body, question.options.len(), &app.t("cli.screen.card.other"), "", "");
                    }
                }
            }
        }
        Kind::Plan { text, .. } => {
            body.push(Line::from(Span::styled(app.t("cli.card.plan"), p.tone(Tone::Strong))));
            let plan = markdown::render(text, inner, p);
            let more = plan.len() > PLAN_LINES;
            body.extend(plan.into_iter().take(PLAN_LINES));
            if more {
                body.push(Line::from(Span::styled("…", p.tone(Tone::Quiet))));
            }
            option(&mut body, 0, &app.t("cli.screen.card.planStart"), "", "");
            option(&mut body, 1, &app.t("cli.screen.card.planKeep"), "", "");
        }
        Kind::Permission { step } => {
            body.extend(wrap_text(
                &app.words.fill("cli.card.permission", &[("step", step)]),
                p.tone(Tone::Strong),
                inner,
                Span::raw(""),
                "",
            ));
            option(&mut body, 0, &app.t("cli.screen.card.allowOnce"), "", "");
            option(&mut body, 1, &app.t("cli.screen.card.allowAlways"), "", "");
            option(&mut body, 2, &app.t("cli.screen.card.refuse"), "", "");
        }
    }
    if !card.own {
        let hint = if card.is_multi() {
            app.t("cli.screen.card.multi")
        } else {
            let keys = if card.choices() <= 1 { "1".to_string() } else { format!("1-{}", card.choices().min(9)) };
            app.words.fill("cli.screen.card.choose", &[("keys", &keys)])
        };
        body.push(Line::from(Span::styled(truncate(&hint, inner), p.tone(Tone::Quiet))));
    }
    body.into_iter()
        .map(|line| {
            let mut spans = vec![bar.clone()];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

/// The commands that match the `/` in the box, drawn just above the rule over the thread.
pub fn commands(f: &mut Frame, app: &App, main: Rect, rule_y: u16) {
    let p = &app.palette;
    let matches = app.command_matches();
    let rows = u16::try_from(matches.len().clamp(1, 8)).unwrap_or(1);
    let rows = rows.min(rule_y.saturating_sub(main.y));
    if rows == 0 {
        return;
    }
    let width = main.width.saturating_sub(2).min(60);
    let rect = Rect::new(main.x + 1, rule_y - rows, width, rows);
    f.render_widget(Clear, rect);
    f.buffer_mut().set_style(rect, p.raised());
    let lines: Vec<Line<'static>> = if matches.is_empty() {
        vec![Line::from(Span::styled(format!(" {}", app.t("cli.screen.cmd.none")), p.tone(Tone::Quiet)))]
    } else {
        let cursor = app.command_cursor.min(matches.len() - 1);
        // The window of rows follows the cursor.
        let first = (cursor + 1).saturating_sub(usize::from(rows));
        matches
            .iter()
            .enumerate()
            .skip(first)
            .take(usize::from(rows))
            .map(|(n, name)| {
                let row = format!(" /{name:<8} ");
                let about = truncate(&app.t(&format!("cli.screen.cmd.{name}")), usize::from(width).saturating_sub(text_width(&row) + 1));
                let mut line = Line::from(vec![Span::styled(row, p.tone(Tone::Accent)), Span::styled(about, p.tone(Tone::Quiet))]);
                if n == cursor {
                    line = line.style(p.selected());
                }
                line
            })
            .collect()
    };
    f.render_widget(Paragraph::new(lines), rect);
}

/// A boxed list in the middle of `area`: the keys, or the quota.
pub fn popup(f: &mut Frame, app: &App, area: Rect) {
    let Some(popup) = &app.popup else { return };
    let p = &app.palette;
    let width = area.width.saturating_sub(4).clamp(20, 72);
    let widest = popup.lines.iter().map(|l| text_width(l)).max().unwrap_or(0).max(text_width(&popup.title));
    let width = u16::try_from(widest + 4).unwrap_or(u16::MAX).clamp(24, width);
    let height = u16::try_from(popup.lines.len() + 4).unwrap_or(u16::MAX).min(area.height.saturating_sub(2)).max(3);
    let rect = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width.min(area.width),
        height.min(area.height),
    );
    f.render_widget(Clear, rect);
    f.buffer_mut().set_style(rect, p.raised());
    let inner = usize::from(rect.width).saturating_sub(4);
    let mut lines = vec![Line::from(Span::styled(truncate(&popup.title, inner), p.tone(Tone::Strong))), Line::default()];
    lines.extend(popup.lines.iter().map(|l| Line::from(Span::styled(truncate(&markdown::clean(l), inner), p.tone(Tone::Text)))));
    let body = Rect::new(rect.x + 2, rect.y + 1, rect.width.saturating_sub(4), rect.height.saturating_sub(2));
    f.render_widget(Paragraph::new(lines), body);
}
