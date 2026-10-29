//! The terminal screen's colours: the person's theme (`look` in the state answer) in true colour, the 16 ANSI colours
//! when the terminal has no more, and attributes only (bold, dim, reverse) when colour is off.

use ratatui::style::{Color, Modifier, Style};
use serde_json::Value;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Colors {
    /// No colour (`NO_COLOR`, `--plain`, a terminal that cannot): bold, dim and reverse only.
    Plain,
    /// The 16 colours every terminal has.
    Ansi,
    /// The theme's own colours.
    True,
}

impl Colors {
    /// What the terminal can show. `color` is `Out::color` (false for `NO_COLOR`, `--plain` and a terminal with no escapes);
    /// a terminal says it has true colour with `COLORTERM`, and Windows Terminal with `WT_SESSION`.
    pub fn detect(color: bool, env: &dyn Fn(&str) -> Option<String>) -> Colors {
        if !color {
            return Colors::Plain;
        }
        let truecolor = env("COLORTERM").is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "truecolor" | "24bit"))
            || env("WT_SESSION").is_some_and(|v| !v.is_empty());
        if truecolor { Colors::True } else { Colors::Ansi }
    }
}

/// The meaning of a piece of text, and nothing about how it is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Text,
    Strong,
    Quiet,
    Accent,
    Good,
    Warn,
    Bad,
    Heading,
    Code,
    Claude,
    Codex,
}

#[derive(Clone, Debug)]
pub struct Palette {
    pub colors: Colors,
    bg: Color,
    s1: Color,
    s2: Color,
    s3: Color,
    ink: Color,
    ink3: Color,
    accent: Color,
    amber: Color,
    danger: Color,
    claude: Color,
    codex: Color,
}

fn hex(text: &str) -> Option<Color> {
    let digits = text.strip_prefix('#').filter(|d| d.len() == 6 && d.bytes().all(|b| b.is_ascii_hexdigit()))?;
    let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some(Color::Rgb(channel(0)?, channel(2)?, channel(4)?))
}

impl Palette {
    /// The colours of `look` (`{vars: {bg, s1, ink, accent, ...}, scheme}`); anything it lacks is Faber's default theme.
    pub fn new(colors: Colors, look: &Value) -> Palette {
        let var = |name: &str, fallback: &str| look["vars"][name].as_str().and_then(hex).or_else(|| hex(fallback)).unwrap_or(Color::Reset);
        Palette {
            colors,
            bg: var("bg", "#0e0e10"),
            s1: var("s1", "#141416"),
            s2: var("s2", "#1a1a1d"),
            s3: var("s3", "#242428"),
            ink: var("ink", "#f3f2ee"),
            ink3: var("ink-3", "#8d8c86"),
            accent: var("accent-text", "#f3f0e8"),
            amber: var("amber", "#f2b13c"),
            danger: var("danger", "#f2766b"),
            claude: var("claude-ink", "#f08a62"),
            codex: var("codex-ink", "#aaa4f4"),
        }
    }

    #[cfg(test)]
    pub fn plain() -> Palette {
        Palette::new(Colors::Plain, &Value::Null)
    }

    fn true_color(&self) -> bool {
        self.colors == Colors::True
    }

    /// The style of a tone. It sets a foreground at most: surfaces are filled first, and text keeps the surface under it.
    pub fn tone(&self, tone: Tone) -> Style {
        let bold = Style::new().add_modifier(Modifier::BOLD);
        match (self.colors, tone) {
            (Colors::True, Tone::Text) => Style::new().fg(self.ink),
            (Colors::True, Tone::Strong) => bold.fg(self.ink),
            (Colors::True, Tone::Quiet) => Style::new().fg(self.ink3),
            (Colors::True, Tone::Accent | Tone::Good) => Style::new().fg(self.accent),
            (Colors::True, Tone::Warn) => Style::new().fg(self.amber),
            (Colors::True, Tone::Bad) => Style::new().fg(self.danger),
            (Colors::True, Tone::Heading) => bold.fg(self.accent),
            (Colors::True, Tone::Code) => Style::new().fg(self.accent),
            (Colors::True, Tone::Claude) => Style::new().fg(self.claude),
            (Colors::True, Tone::Codex) => Style::new().fg(self.codex),
            (Colors::Ansi, Tone::Text) => Style::new(),
            (Colors::Ansi, Tone::Strong) => bold,
            (Colors::Ansi, Tone::Quiet) => Style::new().add_modifier(Modifier::DIM),
            (Colors::Ansi, Tone::Accent) => Style::new().fg(Color::Cyan),
            (Colors::Ansi, Tone::Good) => Style::new().fg(Color::Green),
            (Colors::Ansi, Tone::Warn) => Style::new().fg(Color::Yellow),
            (Colors::Ansi, Tone::Bad) => Style::new().fg(Color::Red),
            (Colors::Ansi, Tone::Heading) => bold.fg(Color::Cyan),
            (Colors::Ansi, Tone::Code) => Style::new().fg(Color::LightBlue),
            (Colors::Ansi, Tone::Claude) => Style::new().fg(Color::LightRed),
            (Colors::Ansi, Tone::Codex) => Style::new().fg(Color::LightMagenta),
            (Colors::Plain, Tone::Strong | Tone::Heading) => bold,
            (Colors::Plain, Tone::Quiet) => Style::new().add_modifier(Modifier::DIM),
            (Colors::Plain, _) => Style::new(),
        }
    }

    pub fn italic(&self, style: Style) -> Style {
        style.add_modifier(Modifier::ITALIC)
    }

    /// The screen's own ground: the theme's background in true colour, the terminal's own otherwise.
    pub fn ground(&self) -> Style {
        if self.true_color() { Style::new().fg(self.ink).bg(self.bg) } else { Style::new() }
    }

    /// The sidebar's ground.
    pub fn side(&self) -> Style {
        if self.true_color() { Style::new().fg(self.ink).bg(self.s1) } else { Style::new() }
    }

    /// Cards, the box and the status bar sit one step above the ground.
    pub fn raised(&self) -> Style {
        if self.true_color() { Style::new().fg(self.ink).bg(self.s2) } else { Style::new() }
    }

    /// A block of code, one step above the ground.
    pub fn code_ground(&self) -> Style {
        if self.true_color() { Style::new().bg(self.s1) } else { Style::new() }
    }

    /// The row the cursor is on: a lighter surface, or reversed colours when there is none.
    pub fn selected(&self) -> Style {
        if self.true_color() { Style::new().bg(self.s3) } else { Style::new().add_modifier(Modifier::REVERSED) }
    }

    /// A rule between areas.
    pub fn rule(&self) -> Style {
        self.tone(Tone::Quiet)
    }

    /// The colour of a vendor's name.
    pub fn vendor(&self, vendor: &str) -> Style {
        match vendor {
            "claude" => self.tone(Tone::Claude),
            "codex" => self.tone(Tone::Codex),
            _ => self.tone(Tone::Text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    }

    #[test]
    fn colour_depth_follows_the_terminal() {
        assert_eq!(Colors::detect(true, &env(&[("COLORTERM", "truecolor")])), Colors::True);
        assert_eq!(Colors::detect(true, &env(&[("COLORTERM", "24bit")])), Colors::True);
        assert_eq!(Colors::detect(true, &env(&[("WT_SESSION", "x")])), Colors::True);
        assert_eq!(Colors::detect(true, &env(&[])), Colors::Ansi);
        assert_eq!(Colors::detect(false, &env(&[("COLORTERM", "truecolor")])), Colors::Plain);
    }

    #[test]
    fn the_theme_paints_in_true_colour_and_only_there() {
        let look = json!({ "vars": { "bg": "#101112", "ink": "#eeeeee", "amber": "#ff9900", "accent-text": "#336699" } });
        let palette = Palette::new(Colors::True, &look);
        assert_eq!(palette.ground().bg, Some(Color::Rgb(0x10, 0x11, 0x12)));
        assert_eq!(palette.tone(Tone::Warn).fg, Some(Color::Rgb(0xff, 0x99, 0x00)));
        assert_eq!(palette.tone(Tone::Accent).fg, Some(Color::Rgb(0x33, 0x66, 0x99)));
        let ansi = Palette::new(Colors::Ansi, &look);
        assert_eq!(ansi.ground().bg, None);
        assert_eq!(ansi.tone(Tone::Warn).fg, Some(Color::Yellow));
        let plain = Palette::new(Colors::Plain, &look);
        for tone in [Tone::Accent, Tone::Good, Tone::Warn, Tone::Bad, Tone::Claude, Tone::Codex, Tone::Code] {
            assert_eq!(plain.tone(tone).fg, None);
        }
        assert!(plain.tone(Tone::Strong).add_modifier.contains(Modifier::BOLD));
        assert!(plain.selected().add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn a_missing_or_odd_colour_falls_to_the_default_theme() {
        let palette = Palette::new(Colors::True, &json!({ "vars": { "bg": "rgba(1,2,3,.5)" } }));
        assert_eq!(palette.ground().bg, Some(Color::Rgb(0x0e, 0x0e, 0x10)));
    }
}
