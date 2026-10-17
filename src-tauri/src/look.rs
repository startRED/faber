//! The look engine: it turns the user's appearance choices into CSS variables. Every colour rule lives here, so the
//! screen never computes a colour and `cargo test` covers the rules with no window. The tokens come from
//! `themes.json` (docs/design/looks.md).

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

type Obj = Map<String, Value>;
type Vars = BTreeMap<String, String>;
type Rgb = [u8; 3];

const WHITE: Rgb = [255, 255, 255];
const BLACK: Rgb = [0, 0, 0];
/// What the accent fill and its text version must reach on `bg`.
const ACCENT_MIN: f64 = 3.0;
const ACCENT_TEXT_MIN: f64 = 4.5;
/// Colours the theme file may set. The meaning colours (amber, Claude, Codex) are never among them.
const SOLID_COLORS: [&str; 8] = ["frame", "bg", "s1", "s2", "s3", "ink", "ink-2", "ink-3"];
const LINE_COLORS: [&str; 3] = ["line", "line-2", "line-3"];
/// The meaning colours in the order that settles a tie, with the error each one gives.
const MEANINGS: [(&str, &str); 3] =
    [("amber", "error.look.nearAmber"), ("claude", "error.look.nearClaude"), ("codex", "error.look.nearCodex")];

/// An error the screen can show: a translation key and, sometimes, the field or the pairs it is about.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Failure {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Failure {
    pub fn key(key: &str) -> Self {
        Failure { key: key.into(), detail: None }
    }

    fn with(key: &str, detail: impl Into<String>) -> Self {
        Failure { key: key.into(), detail: Some(detail.into()) }
    }

    fn invalid(field: impl Into<String>) -> Self {
        Failure::with("error.look.fileInvalid", field)
    }
}

/// Engine errors are plain keys.
impl From<String> for Failure {
    fn from(key: String) -> Self {
        Failure { key, detail: None }
    }
}

// ---- the tokens ----

fn token(pointer: &str) -> &'static Value {
    static TOKENS: OnceLock<Value> = OnceLock::new();
    TOKENS
        .get_or_init(|| serde_json::from_str(include_str!("themes.json")).expect("themes.json is valid JSON"))
        .pointer(pointer)
        .unwrap_or_else(|| panic!("themes.json has no {pointer}"))
}

fn strings(v: &Value) -> Vars {
    v.as_object().into_iter().flatten().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect()
}

fn list(pointer: &str) -> Vec<&'static str> {
    token(pointer).as_array().into_iter().flatten().filter_map(Value::as_str).collect()
}

fn number(v: &Value) -> f64 {
    v.as_f64().expect("themes.json number")
}

/// Every token of a theme or preset: the Sala base with the overrides of `id` on top. None for an unknown id.
fn css_of(id: &str) -> Option<Vars> {
    let over = token("/css/themes").get(id).or_else(|| token("/css/presets").get(id))?;
    let mut vars = strings(token("/css/base"));
    vars.extend(strings(over));
    Some(vars)
}

fn default_base() -> &'static str {
    token("/default").as_str().expect("themes.json default")
}

/// Ids of the themes and presets, in the order the screen lists them.
fn bases() -> Vec<&'static str> {
    ["/user-controls/themes", "/user-controls/presets"].iter().flat_map(|p| list(p)).collect()
}

/// The keys of an object in `themes.json`: the allowed values of a choice.
fn choices(pointer: &str) -> Vec<&'static str> {
    token(pointer).as_object().into_iter().flatten().map(|(k, _)| k.as_str()).collect()
}

pub fn theme_extension() -> &'static str {
    token("/theme-file/extension").as_str().expect("themes.json extension")
}

pub fn max_theme_bytes() -> u64 {
    token("/theme-file/max-bytes").as_u64().expect("themes.json max-bytes")
}

// ---- colours ----

fn parse_hex(s: &str) -> Option<Rgb> {
    let digits = s.strip_prefix('#').filter(|d| d.len() == 6 && d.bytes().all(|b| b.is_ascii_hexdigit()))?;
    let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn rgb_of(vars: &Vars, key: &str) -> Rgb {
    vars.get(key).and_then(|v| parse_hex(v)).unwrap_or_else(|| panic!("{key} is not #rrggbb"))
}

fn luminance(c: Rgb) -> f64 {
    let [r, g, b] = c.map(|v| {
        let v = f64::from(v) / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// The WCAG contrast ratio.
fn ratio(a: Rgb, b: Rgb) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let channel = |i: usize| (f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * t).round().clamp(0.0, 255.0) as u8;
    [channel(0), channel(1), channel(2)]
}

/// Moves `c` toward `to` in 5% steps until it reaches `min` on every surface in `on`, stopping at `to`.
fn fix(c: Rgb, on: &[Rgb], min: f64, to: Rgb) -> Rgb {
    (0..=20).map(|step| mix(c, to, f64::from(step) / 20.0)).find(|x| on.iter().all(|surface| ratio(*x, *surface) >= min)).unwrap_or(to)
}

/// Pure white or pure black, whichever reads better on `bg`.
fn white_reads_better(bg: Rgb) -> bool {
    ratio(bg, WHITE) >= ratio(bg, BLACK)
}

/// The HSL hue in degrees. Only for colours that have one (see `near_meaning`).
fn hue(c: Rgb) -> f64 {
    let [r, g, b] = c.map(|v| f64::from(v) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let h = if max == r {
        ((g - b) / d) % 6.0
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0 + 360.0) % 360.0
}

/// The hue block, as a pure function of the hue: the error for the nearest meaning colour, if the hue is less than
/// the allowed distance from any of them, measured the short way round the circle.
fn hue_block(h: f64) -> Option<Failure> {
    let block = token("/validator/accent-hue-block");
    let within = number(&block["within-degrees"]);
    let mut nearest: Option<(f64, &str)> = None;
    for (name, key) in MEANINGS {
        let apart = (h - number(&block["near"][name])).abs();
        let d = apart.min(360.0 - apart);
        if d < within && nearest.is_none_or(|(best, _)| d < best) {
            nearest = Some((d, key));
        }
    }
    nearest.map(|(_, key)| Failure::key(key))
}

/// A colour whose channels are close together has no hue to confuse with a meaning colour.
fn near_meaning(c: Rgb) -> Option<Failure> {
    let spread = c.iter().max().copied().unwrap_or(0) - c.iter().min().copied().unwrap_or(0);
    if f64::from(spread) < number(token("/validator/accent-hue-block/neutral-spread")) {
        return None;
    }
    hue_block(hue(c))
}

struct Accent {
    fill: Rgb,
    text: Rgb,
    ink: Rgb,
}

/// The validator for an accent: swatches, the colour picker and theme files all come through here.
fn accent_for(candidate: Rgb, bg: Rgb) -> Result<Accent, Failure> {
    if let Some(f) = near_meaning(candidate) {
        return Err(f);
    }
    let to = if white_reads_better(bg) { WHITE } else { BLACK };
    let fill = fix(candidate, &[bg], ACCENT_MIN, to);
    let text = fix(fill, &[bg], ACCENT_TEXT_MIN, to);
    // Fixing the contrast can move the hue by a rounding step, so the result is checked once more.
    if let Some(f) = near_meaning(fill).or_else(|| near_meaning(text)) {
        return Err(f);
    }
    let ink = if ratio(fill, BLACK) >= ratio(fill, WHITE) { BLACK } else { WHITE };
    Ok(Accent { fill, text, ink })
}

// ---- the appearance ----

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Effects {
    pub ambient: bool,
    pub texture: bool,
    pub glow: bool,
}

const ALL_EFFECTS: Effects = Effects { ambient: true, texture: true, glow: true };

/// The choices behind a look. The saved appearance and the theme file share this shape; only `base` is required.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Appearance {
    pub base: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ground: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_font: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heading_font: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corners: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effects: Option<Effects>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub colors: BTreeMap<String, String>,
}

impl Appearance {
    /// A theme or preset exactly as designed.
    pub fn of(base: &str) -> Self {
        Appearance {
            base: base.into(),
            ground: None,
            accent: None,
            text_font: None,
            heading_font: None,
            text_size: None,
            density: None,
            corners: None,
            motion: None,
            effects: None,
            colors: BTreeMap::new(),
        }
    }

    /// Reads the saved shape, or with `header` the theme file's three extra fields as well. Every problem names the
    /// field. The meaning colours in `colors` are dropped, not refused.
    pub fn parse(map: &Obj, header: bool) -> Result<Self, Failure> {
        const FIELDS: [&str; 11] =
            ["base", "ground", "accent", "textFont", "headingFont", "textSize", "density", "corners", "motion", "effects", "colors"];
        if let Some(unknown) =
            map.keys().find(|k| !FIELDS.contains(&k.as_str()) && !(header && ["format", "version", "name"].contains(&k.as_str())))
        {
            return Err(Failure::invalid(unknown));
        }
        let text = |field: &str, allowed: &[&str]| -> Result<Option<String>, Failure> {
            match map.get(field) {
                None => Ok(None),
                Some(Value::String(s)) if allowed.contains(&s.as_str()) => Ok(Some(s.clone())),
                Some(_) => Err(Failure::invalid(field)),
            }
        };
        let numeric = |field: &str, ok: &dyn Fn(f64) -> bool| -> Result<Option<f64>, Failure> {
            match map.get(field) {
                None => Ok(None),
                Some(v) => v.as_f64().filter(|n| ok(*n)).map(Some).ok_or_else(|| Failure::invalid(field)),
            }
        };
        let one_of = |options: Vec<f64>| move |n: f64| options.iter().any(|o| (o - n).abs() < 1e-9);
        let size = token("/user-controls/text-size");
        let (min, max, step) = (number(&size["min"]), number(&size["max"]), number(&size["step"]));
        let on_step = move |n: f64| (min..=max).contains(&n) && ((n - min) / step - ((n - min) / step).round()).abs() < 1e-9;

        let base = text("base", &bases())?.ok_or_else(|| Failure::invalid("base"))?;
        let accent = match map.get("accent") {
            None => None,
            Some(Value::String(s)) if parse_hex(s).is_some() => Some(s.to_ascii_lowercase()),
            Some(_) => return Err(Failure::invalid("accent")),
        };
        let numbers = |pointer: &str| token(pointer).as_array().into_iter().flatten().map(number).collect::<Vec<_>>();
        Ok(Appearance {
            base,
            ground: text("ground", &list("/user-controls/grounds"))?,
            accent,
            text_font: text("textFont", &choices("/user-controls/text-fonts"))?,
            heading_font: text("headingFont", &choices("/user-controls/heading-fonts"))?,
            text_size: numeric("textSize", &on_step)?,
            density: numeric("density", &one_of(numbers("/user-controls/density")))?,
            corners: numeric("corners", &one_of(numbers("/user-controls/corners")))?,
            motion: text("motion", &choices("/user-controls/motion"))?,
            effects: parse_effects(map.get("effects"))?,
            colors: parse_colors(map.get("colors"))?,
        })
    }

    pub fn to_map(&self) -> Obj {
        match serde_json::to_value(self) {
            Ok(Value::Object(map)) => map,
            _ => Obj::new(),
        }
    }
}

fn parse_effects(value: Option<&Value>) -> Result<Option<Effects>, Failure> {
    let Some(value) = value else { return Ok(None) };
    let Value::Object(map) = value else { return Err(Failure::invalid("effects")) };
    if let Some(unknown) = map.keys().find(|k| !["ambient", "texture", "glow"].contains(&k.as_str())) {
        return Err(Failure::invalid(format!("effects.{unknown}")));
    }
    let flag = |name: &str| match map.get(name) {
        None => Ok(true),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(Failure::invalid(format!("effects.{name}"))),
    };
    Ok(Some(Effects { ambient: flag("ambient")?, texture: flag("texture")?, glow: flag("glow")? }))
}

fn is_rgba(s: &str) -> bool {
    let inner = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')'));
    let Some(inner) = inner else { return false };
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    let channel = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u16>().is_ok_and(|v| v <= 255);
    let alpha = |p: &str| p.bytes().all(|b| b.is_ascii_digit() || b == b'.') && p.parse::<f64>().is_ok_and(|v| (0.0..=1.0).contains(&v));
    matches!(parts.len(), 3 | 4) && parts[..3].iter().all(|p| channel(p)) && parts.get(3).is_none_or(|p| alpha(p))
}

fn parse_colors(value: Option<&Value>) -> Result<BTreeMap<String, String>, Failure> {
    let Some(value) = value else { return Ok(BTreeMap::new()) };
    let Value::Object(map) = value else { return Err(Failure::invalid("colors")) };
    let mut colors = BTreeMap::new();
    for (key, v) in map {
        if MEANINGS.iter().any(|(name, _)| key.starts_with(name)) {
            continue;
        }
        let line = LINE_COLORS.contains(&key.as_str());
        if !line && !SOLID_COLORS.contains(&key.as_str()) {
            return Err(Failure::invalid(format!("colors.{key}")));
        }
        let color =
            v.as_str().map(str::to_ascii_lowercase).filter(|c| !c.contains("var(") && (parse_hex(c).is_some() || (line && is_rgba(c))));
        colors.insert(key.clone(), color.ok_or_else(|| Failure::invalid(format!("colors.{key}")))?);
    }
    Ok(colors)
}

// ---- the look ----

/// One of the required contrast pairs, measured.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    pub a: String,
    pub b: String,
    pub ratio: f64,
    pub min: f64,
    pub pass: bool,
}

/// What the screen applies: the CSS variables (without the leading `--`), the motion level the user chose, the
/// effect switches, the contrast checks and any notices. `appearance` is the choices with every field filled in
/// from the base, so the screen can show which control is on.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Look {
    pub vars: Vars,
    pub scheme: &'static str,
    pub motion: String,
    pub effects: Effects,
    pub checks: Vec<Check>,
    pub notices: Vec<String>,
    pub appearance: Appearance,
}

impl Look {
    /// A look with a failing pair cannot be saved or applied. The failure names every failing pair. The plain inks
    /// also draw on `frame` (the title bar and the sidebar), so they are held to the same minimum there.
    pub fn failing(&self) -> Option<Failure> {
        let frame = rgb_of(&self.vars, "frame");
        let on_frame = list("/validator/required-pairs/plain-inks")
            .into_iter()
            .filter(|ink| ratio(rgb_of(&self.vars, ink), frame) < ink_min())
            .map(|ink| format!("{ink} / frame"));
        let names: Vec<String> = self.checks.iter().filter(|c| !c.pass).map(|c| format!("{} / {}", c.a, c.b)).chain(on_frame).collect();
        (!names.is_empty()).then(|| Failure::with("error.look.pairFailed", names.join(", ")))
    }
}

fn ink_min() -> f64 {
    number(token("/validator/required-pairs/ink-min"))
}

fn required_checks(vars: &Vars) -> Vec<Check> {
    let pairs = token("/validator/required-pairs");
    let inks = [list("/validator/required-pairs/plain-inks"), list("/validator/required-pairs/meaning-inks")].concat();
    let mut wanted: Vec<(&str, &str, f64)> = Vec::new();
    for ink in &inks {
        for surface in list("/validator/required-pairs/surfaces") {
            wanted.push((ink, surface, ink_min()));
        }
    }
    for key in ["accent", "accent-ink"] {
        let p = &pairs[key];
        wanted.push((p[0].as_str().expect("pair"), p[1].as_str().expect("pair"), number(&p[2])));
    }
    wanted
        .into_iter()
        .map(|(a, b, min)| {
            let r = ratio(rgb_of(vars, a), rgb_of(vars, b));
            Check { a: a.into(), b: b.into(), ratio: (r * 100.0).round() / 100.0, min, pass: r >= min }
        })
        .collect()
}

/// Whole pixels from a value like `12px`.
fn px(value: &str) -> f64 {
    value.trim_end_matches("px").parse().unwrap_or(0.0)
}

pub fn look(a: &Appearance) -> Result<Look, Failure> {
    let mut vars = css_of(&a.base).ok_or_else(|| Failure::invalid("base"))?;
    let own = vars.clone();
    let defaults = &token("/defaults")[a.base.as_str()];
    let mut notices = Vec::new();

    // A ground replaces the surfaces, lines, inks and meaning colours; the base keeps everything else.
    let ground = a.ground.as_deref().filter(|g| *g != a.base);
    if let Some(g) = ground.and_then(css_of) {
        for key in list("/validator/ground-keys") {
            if let Some(v) = g.get(key) {
                vars.insert(key.into(), v.clone());
            }
        }
    }

    // Colours of the user. A new `bg` brings the meaning colours of a dark or a light ground and pushes the plain
    // inks until they read on every surface; anything else that fails is reported by the checks.
    let bg_before = vars["bg"].clone();
    vars.extend(a.colors.clone());
    let bg_changed = vars["bg"] != bg_before;
    if bg_changed {
        let bg = rgb_of(&vars, "bg");
        let (source, to) = if white_reads_better(bg) { ("sala", WHITE) } else { ("manha", BLACK) };
        let source = css_of(source).expect("sala and manha exist");
        for key in list("/validator/meaning-keys") {
            vars.insert(key.into(), source[key].clone());
        }
        let surfaces: Vec<Rgb> = list("/validator/required-pairs/surfaces").into_iter().map(|k| rgb_of(&vars, k)).collect();
        for key in list("/validator/required-pairs/plain-inks") {
            let pushed = fix(rgb_of(&vars, key), &surfaces, ink_min(), to);
            vars.insert(key.into(), hex(pushed));
        }
    }

    // The accent is validated when the user picks one, and again when a new ground or `bg` could make the base's own unreadable.
    if a.accent.is_some() || ground.is_some() || bg_changed {
        let candidate = a.accent.as_deref().and_then(parse_hex).unwrap_or_else(|| rgb_of(&vars, "accent"));
        let accent = accent_for(candidate, rgb_of(&vars, "bg"))?;
        if a.accent.is_some() && accent.fill != candidate {
            notices.push("look.accentAdjusted".to_string());
        }
        vars.insert("accent".into(), hex(accent.fill));
        vars.insert("accent-text".into(), hex(accent.text));
        vars.insert("accent-ink".into(), hex(accent.ink));
    }

    if let Some(font) = &a.text_font {
        vars.insert("font-ui".into(), token("/user-controls/text-fonts")[font.as_str()].as_str().expect("font stack").into());
    }
    if let Some(font) = &a.heading_font {
        vars.extend(strings(&token("/user-controls/heading-fonts")[font.as_str()]));
    }
    if let Some(size) = a.text_size {
        // Every interface size moves with the text size and keeps the offset its theme gave it.
        let delta = size - px(&own["fs-ui"]);
        for key in ["fs-xs", "fs-sm", "fs-ui", "fs-read", "fs-lead", "nav-size"] {
            vars.insert(key.into(), format!("{}px", px(&own[key]) + delta));
        }
    }
    if let Some(density) = a.density {
        vars.insert("dens".into(), density.to_string());
    }
    if let Some(corners) = a.corners {
        for key in ["r-xs", "r-sm", "r-md", "r-lg", "r-pill", "r-stage"] {
            vars.insert(key.into(), format!("{}px", (px(&vars[key]) * corners).round()));
        }
    }

    let effects = a.effects.unwrap_or(ALL_EFFECTS);
    if !effects.texture {
        vars.insert("grain".into(), "0".into());
        vars.insert("bench".into(), "none".into());
    }
    if !effects.glow {
        for key in ["focus-light", "amber-light"] {
            vars.insert(key.into(), "0 0 0 0 transparent".into());
        }
    }
    if !effects.ambient {
        for key in ["amb-beam", "amb-loom", "amb-flow", "amb-hole"] {
            vars.insert(key.into(), "none".into());
        }
    }

    let checks = required_checks(&vars);
    let text = |field: &Option<String>, key: &str| field.clone().or_else(|| defaults[key].as_str().map(String::from));
    let appearance = Appearance {
        base: a.base.clone(),
        ground: Some(ground.unwrap_or(&a.base).to_string()),
        accent: Some(a.accent.clone().unwrap_or_else(|| own["accent"].clone())),
        text_font: text(&a.text_font, "textFont"),
        heading_font: text(&a.heading_font, "headingFont"),
        text_size: Some(a.text_size.unwrap_or_else(|| px(&own["fs-ui"]))),
        density: Some(a.density.unwrap_or_else(|| own["dens"].parse().unwrap_or(1.0))),
        corners: Some(a.corners.unwrap_or(1.0)),
        motion: text(&a.motion, "motion"),
        effects: Some(effects),
        colors: a.colors.clone(),
    };
    let scheme = if white_reads_better(rgb_of(&vars, "bg")) { "dark" } else { "light" };
    Ok(Look { vars, scheme, motion: appearance.motion.clone().unwrap_or_else(|| "full".into()), effects, checks, notices, appearance })
}

/// A look that can be saved: the choices parse, the accent passes the validator and every required pair passes.
pub fn validated(map: &Obj) -> Result<(Appearance, Look), Failure> {
    let appearance = Appearance::parse(map, false)?;
    let look = look(&appearance)?;
    match look.failing() {
        Some(f) => Err(f),
        None => Ok((appearance, look)),
    }
}

/// The look for the preview: failing pairs are reported in the look, not as an error.
pub fn preview(map: &Obj) -> Result<Look, Failure> {
    look(&Appearance::parse(map, false)?)
}

/// The look of the saved appearance. An absent, old or failing appearance gives the default look, so the app never
/// fails to start because of its look.
pub fn stored(appearance: Option<&Obj>) -> Look {
    saved_look(appearance).unwrap_or_else(|| look(&Appearance::of(default_base())).expect("the default look is valid"))
}

fn saved_look(appearance: Option<&Obj>) -> Option<Look> {
    appearance.and_then(|map| match validated(map) {
        Ok((_, look)) => Some(look),
        Err(f) => {
            eprintln!("faber: the saved appearance was ignored: {} {}", f.key, f.detail.unwrap_or_default());
            None
        }
    })
}

/// The saved appearance as the screen should see it: an old or failing one is absent, like the look it falls back to,
/// so the Settings controls never build a save from fields that cannot validate.
pub fn usable(appearance: Option<Obj>) -> Option<Obj> {
    appearance.filter(|map| saved_look(Some(map)).is_some())
}

// ---- the theme file ----

/// What `open_theme` hands to the screen: the file's name and its choices.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThemeFile {
    pub name: String,
    #[serde(flatten)]
    pub appearance: Appearance,
}

fn valid_name(name: &str) -> bool {
    (1..=80).contains(&name.chars().count())
}

/// Reads a theme file. A file holds choices, never CSS: `var(` is refused and so are unknown keys, and a refused
/// accent refuses the whole file. Failing pairs do not: the preview names them.
pub fn parse_theme_file(bytes: &[u8]) -> Result<ThemeFile, Failure> {
    if bytes.len() as u64 > max_theme_bytes() {
        return Err(Failure::key("error.look.fileTooBig"));
    }
    let Ok(Value::Object(map)) = serde_json::from_slice::<Value>(bytes) else { return Err(Failure::invalid("file")) };
    if map.get("format").and_then(Value::as_str) != Some("faber-theme") {
        return Err(Failure::invalid("format"));
    }
    if map.get("version").and_then(Value::as_u64) != Some(1) {
        return Err(Failure::invalid("version"));
    }
    let name = map.get("name").and_then(Value::as_str).filter(|n| valid_name(n)).ok_or_else(|| Failure::invalid("name"))?;
    let appearance = Appearance::parse(&map, true)?;
    look(&appearance)?;
    Ok(ThemeFile { name: name.into(), appearance })
}

#[derive(Serialize)]
struct FileOut<'a> {
    format: &'static str,
    version: u8,
    name: &'a str,
    #[serde(flatten)]
    appearance: &'a Appearance,
}

/// The theme file for a look that can be saved, with its keys in a stable order.
pub fn write_theme_file(appearance: &Obj, name: &str) -> Result<String, Failure> {
    if !valid_name(name) {
        return Err(Failure::invalid("name"));
    }
    let (appearance, _) = validated(appearance)?;
    let out = FileOut { format: "faber-theme", version: 1, name, appearance: &appearance };
    Ok(serde_json::to_string_pretty(&out).expect("plain data always serializes") + "\n")
}

/// Writes `target` through a sibling file and renames it over the target only when the whole write worked, so a full disk
/// or a crash halfway leaves the file that was there as it was.
fn replace_file(target: &Path, write: impl FnOnce(&mut std::fs::File) -> io::Result<()>) -> io::Result<()> {
    let mut name = target.as_os_str().to_owned();
    name.push(".faber-tmp");
    let temp = PathBuf::from(name);
    let done = std::fs::File::create(&temp).and_then(|mut file| {
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, target)
    });
    if done.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    done
}

/// Saves a theme file the person picked a place for. The words of a failure are ours; the system's stay out of the screen.
pub fn export_theme(target: &Path, json: &str) -> Result<(), Failure> {
    replace_file(target, |file| file.write_all(json.as_bytes())).map_err(|_| Failure::key("error.theme.write"))
}

/// Reads a theme file the person picked. One byte past the limit is enough to refuse it without loading all of it.
pub fn import_theme(path: &Path) -> Result<ThemeFile, Failure> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(max_theme_bytes() + 1).read_to_end(&mut bytes))
        .map_err(|_| Failure::key("error.theme.read"))?;
    parse_theme_file(&bytes)
}

/// The window's first paint: the look's `frame` colour.
pub fn frame_color(look: &Look) -> Rgb {
    rgb_of(&look.vars, "frame")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rgb(h: &str) -> Rgb {
        parse_hex(h).expect("a test colour")
    }

    #[test]
    fn the_hue_block_passes_its_boundaries_and_refuses_just_inside_the_bands() {
        for h in [64.0, 221.0, 269.0, 350.0, 349.0, 100.0] {
            assert!(hue_block(h).is_none(), "{h} should pass");
        }
        for h in [63.9, 221.1, 268.9, 350.1, 355.0, 5.0, 0.0] {
            assert!(hue_block(h).is_some(), "{h} should be refused");
        }
        // The bands of Claude and amber join: 39 is 25 from Claude and 1 from amber.
        assert_eq!(hue_block(39.0).unwrap().key, "error.look.nearAmber");
    }

    #[test]
    fn a_hue_inside_two_bands_names_the_nearest_meaning_colour() {
        assert_eq!(hue_block(20.0).unwrap().key, "error.look.nearClaude");
        assert_eq!(hue_block(30.0).unwrap().key, "error.look.nearAmber");
        assert_eq!(hue_block(27.0).unwrap().key, "error.look.nearAmber", "a tie goes to amber");
        assert_eq!(hue_block(240.0).unwrap().key, "error.look.nearCodex");
    }

    #[test]
    fn the_hue_block_on_real_colours() {
        let key = |c: &str| near_meaning(rgb(c)).map(|f| f.key);
        assert_eq!(key("#f2b13c").as_deref(), Some("error.look.nearAmber"));
        assert_eq!(key("#e0673a").as_deref(), Some("error.look.nearClaude"));
        assert_eq!(key("#8f87ea").as_deref(), Some("error.look.nearCodex"));
        assert!(key("#5a4a2a").is_some(), "a dark olive brown is amber's hue");
        for pass in ["#5fb36a", "#4f9dde", "#f3f0e8"] {
            assert_eq!(key(pass), None, "{pass}");
        }
    }

    #[test]
    fn a_hex_colour_is_six_digits_after_a_hash_and_the_wcag_ratio_and_the_fix_behave() {
        assert_eq!(parse_hex("#a1B2c3"), Some([0xa1, 0xb2, 0xc3]));
        for bad in ["#abc", "a1b2c3", "#12345g", "#1234567", ""] {
            assert_eq!(parse_hex(bad), None, "{bad:?}");
        }
        assert_eq!(hex([1, 2, 255]), "#0102ff");
        assert!((ratio(WHITE, BLACK) - 21.0).abs() < 1e-9);
        assert_eq!(ratio(rgb("#555555"), rgb("#555555")), 1.0);
        // The fix moves in 5% steps toward the target, leaves a colour that already passes alone, and stops at the target.
        let grey = rgb("#555555");
        let fixed = fix(grey, &[BLACK], 4.5, WHITE);
        assert!(ratio(fixed, BLACK) >= 4.5 && fixed != grey);
        assert_eq!(fix(WHITE, &[BLACK], 4.5, WHITE), WHITE);
        assert_eq!(fix(grey, &[BLACK], 100.0, WHITE), WHITE);
    }

    #[test]
    fn the_saved_shape_reads_back_and_every_refusal_names_its_field() {
        let saved = json!({ "base": "oficina", "accent": "#ABCDEF", "textSize": 15.5, "density": 0.8, "effects": { "glow": false } });
        let a = Appearance::parse(saved.as_object().unwrap(), false).unwrap();
        assert_eq!((a.base.as_str(), a.accent.as_deref(), a.text_size, a.density), ("oficina", Some("#abcdef"), Some(15.5), Some(0.8)));
        assert_eq!(a.effects, Some(Effects { ambient: true, texture: true, glow: false }), "a flag left out is on");
        assert_eq!(Appearance::parse(&a.to_map(), false).unwrap(), a, "to_map and parse are inverse");
        for (bad, field) in
            [(json!({}), "base"), (json!({ "base": "sala", "mode": "dark" }), "mode"), (json!({ "base": "sala", "corners": 2 }), "corners")]
        {
            let refusal = Appearance::parse(bad.as_object().unwrap(), false).unwrap_err();
            assert_eq!((refusal.key.as_str(), refusal.detail.as_deref()), ("error.look.fileInvalid", Some(field)), "{bad}");
        }
        // The three header fields belong to the theme file only.
        let header = json!({ "base": "sala", "format": "faber-theme", "version": 1, "name": "x" });
        assert_eq!(Appearance::parse(header.as_object().unwrap(), false).unwrap_err().detail.as_deref(), Some("format"));
        assert!(Appearance::parse(header.as_object().unwrap(), true).is_ok());
    }

    fn sala() -> Appearance {
        Appearance::of("sala")
    }

    fn colored(base: &str, colors: &[(&str, &str)]) -> Appearance {
        Appearance { colors: colors.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), ..Appearance::of(base) }
    }

    fn accented(base: &str, accent: &str) -> Appearance {
        Appearance { accent: Some(accent.into()), ..Appearance::of(base) }
    }

    fn failing_names(look: &Look) -> Vec<String> {
        look.checks.iter().filter(|c| !c.pass).map(|c| format!("{} / {}", c.a, c.b)).collect()
    }

    fn ratio_of(look: &Look, a: &str, b: &str) -> f64 {
        ratio(rgb_of(&look.vars, a), rgb_of(&look.vars, b))
    }

    #[test]
    fn every_factory_look_passes_all_twenty_six_pairs_and_sets_every_variable() {
        let base_keys: Vec<String> = strings(token("/css/base")).into_keys().collect();
        for id in bases() {
            let look = look(&Appearance::of(id)).unwrap();
            assert_eq!(look.checks.len(), 26, "{id}");
            assert_eq!(failing_names(&look), Vec::<String>::new(), "{id}");
            assert_eq!(look.vars.keys().cloned().collect::<Vec<_>>(), base_keys, "{id}: a look must set every variable");
        }
    }

    #[test]
    fn the_contrast_table_is_recomputed_from_the_css_section() {
        let mut lowest_plain = f64::MAX;
        let mut lowest_meaning = f64::MAX;
        for id in bases() {
            let look = look(&Appearance::of(id)).unwrap();
            let table = token("/contrast").get(id).and_then(Value::as_object).unwrap_or_else(|| panic!("no table for {id}"));
            assert_eq!(table.len(), 26, "{id}");
            for c in &look.checks {
                let listed = table[&format!("{} / {}", c.a, c.b)].as_f64().unwrap();
                assert!((listed - c.ratio).abs() <= 0.01, "{id}: {} / {} lists {listed}, measures {}", c.a, c.b, c.ratio);
                if c.a.starts_with("ink") {
                    lowest_plain = lowest_plain.min(c.ratio);
                } else if c.a.ends_with("-ink") && c.a != "accent-ink" {
                    lowest_meaning = lowest_meaning.min(c.ratio);
                }
            }
        }
        assert_eq!((lowest_plain, lowest_meaning), (4.59, 5.39));
        let sala = look(&sala()).unwrap();
        assert_eq!(sala.vars["accent-ink"], "#0e0e10", "a factory look keeps its own accent ink");
        assert_eq!(sala.checks.last().map(|c| c.ratio), Some(16.93));
    }

    #[test]
    fn white_plain_inks_on_a_grey_s3_pass_alone_but_the_amber_ink_refuses_the_look() {
        let a = colored("sala", &[("ink", "#ffffff"), ("ink-2", "#ffffff"), ("ink-3", "#ffffff"), ("s3", "#666666")]);
        let look = look(&a).unwrap();
        assert!((ratio_of(&look, "ink", "s3") - 5.74).abs() < 0.005);
        assert!((ratio_of(&look, "amber-ink", "s3") - 3.68).abs() < 0.005);
        let refusal = validated(&a.to_map()).unwrap_err();
        assert_eq!(refusal.key, "error.look.pairFailed");
        assert!(refusal.detail.unwrap().contains("amber-ink / s3"));
        assert!(!failing_names(&look).iter().any(|n| n.starts_with("ink")));
    }

    #[test]
    fn a_failing_pair_is_named_and_blocks_the_look() {
        let cases = [
            (colored("sala", &[("bg", "#6b6b6b")]), "amber-ink / bg"),
            (colored("sala", &[("s3", "#f3f2ee")]), "ink / s3"),
            (colored("sala", &[("s1", "#bab9b3")]), "ink-2 / s1"),
        ];
        for (a, pair) in cases {
            assert!(failing_names(&look(&a).unwrap()).contains(&pair.to_string()), "{pair}");
            let refusal = validated(&a.to_map()).unwrap_err();
            assert_eq!(refusal.key, "error.look.pairFailed", "{pair}");
            assert!(refusal.detail.unwrap().contains(pair), "{pair}");
            assert!(preview(&a.to_map()).is_ok(), "the preview still shows a failing look: {pair}");
        }
    }

    #[test]
    fn the_contrast_fix_reaches_3_to_1_and_4_5_to_1_and_a_fix_inside_the_hue_block_is_refused() {
        let fixed = look(&accented("sala", "#0b5d3b")).unwrap();
        assert!(ratio_of(&fixed, "accent", "bg") >= 3.0);
        assert!(ratio_of(&fixed, "accent-text", "bg") >= 4.5);
        assert_eq!(fixed.notices, ["look.accentAdjusted"]);
        assert!(look(&accented("sala", "#8fdcb8")).unwrap().notices.is_empty(), "a readable accent is not adjusted");
        // #001b54 passes the hue block (220.7 degrees), but lightening it moves it to 221.4, inside Codex's band.
        assert!(near_meaning(rgb("#001b54")).is_none());
        assert_eq!(look(&accented("sala", "#001b54")).unwrap_err().key, "error.look.nearCodex");
    }

    #[test]
    fn a_picked_accent_gets_black_or_white_ink() {
        assert_eq!(look(&accented("sala", "#f3f0e8")).unwrap().vars["accent-ink"], "#000000");
        assert_eq!(look(&accented("manha", "#28604f")).unwrap().vars["accent-ink"], "#ffffff");
    }

    #[test]
    fn a_ground_replaces_the_surfaces_and_the_accent_is_checked_again() {
        let light = look(&Appearance { ground: Some("manha".into()), ..sala() }).unwrap();
        assert_eq!(light.vars["bg"], "#f3f5f0");
        assert_eq!(light.scheme, "light");
        assert_eq!(failing_names(&light), Vec::<String>::new());
        assert_ne!(light.vars["accent"], "#f3f0e8", "Sala's light accent cannot stay on a light ground");
        let same = look(&Appearance { ground: Some("sala".into()), ..sala() }).unwrap();
        assert_eq!(same, look(&sala()).unwrap(), "a ground equal to the base changes nothing");
    }

    #[test]
    fn a_new_bg_brings_the_meaning_colours_of_a_matching_ground_and_pushes_the_plain_inks() {
        let light = [("frame", "#f0ebe0"), ("bg", "#faf6ec"), ("s1", "#f4efe2"), ("s2", "#fffdf7"), ("s3", "#f0e9d8")];
        let look_light = look(&colored("sala", &light)).unwrap();
        assert_eq!(look_light.vars["amber-ink"], css_of("manha").unwrap()["amber-ink"]);
        assert_eq!(failing_names(&look_light), Vec::<String>::new());
        let dark = look(&colored("oficina", &[("bg", "#101826")])).unwrap();
        assert_eq!(dark.vars["amber"], css_of("sala").unwrap()["amber"], "Oficina's own amber is replaced by Sala's");
        // Only a changed bg moves the meaning colours: s2 alone leaves them alone.
        assert_eq!(look(&colored("oficina", &[("s2", "#202020")])).unwrap().vars["amber"], css_of("oficina").unwrap()["amber"]);
    }

    #[test]
    fn the_choices_become_tokens() {
        let a = Appearance {
            text_font: Some("plex".into()),
            heading_font: Some("book".into()),
            text_size: Some(15.0),
            density: Some(0.8),
            corners: Some(1.6),
            effects: Some(Effects { ambient: false, texture: false, glow: false }),
            ..sala()
        };
        let look = look(&a).unwrap();
        let v = &look.vars;
        assert_eq!(v["font-ui"], "\"IBM Plex Sans\",system-ui,sans-serif");
        assert_eq!((v["font-display"].as_str(), v["em-style"].as_str()), ("\"Newsreader\",Georgia,serif", "italic"));
        assert_eq!((v["fs-ui"].as_str(), v["fs-read"].as_str(), v["dens"].as_str()), ("15px", "16.5px", "0.8"));
        assert_eq!((v["r-md"].as_str(), v["r-xs"].as_str()), ("19px", "10px"));
        assert_eq!((v["grain"].as_str(), v["bench"].as_str()), ("0", "none"));
        assert_eq!(v["focus-light"], "0 0 0 0 transparent");
        assert_eq!((v["amb-beam"].as_str(), v["amb-loom"].as_str(), v["amb-flow"].as_str()), ("none", "none", "none"));
        assert_eq!(v["amb-hole"], "none");
        assert_eq!(look.effects, Effects { ambient: false, texture: false, glow: false });
        let square = self::look(&Appearance { corners: Some(0.0), ..sala() }).unwrap().vars;
        assert_eq!((square["r-md"].as_str(), square["r-pill"].as_str()), ("0px", "0px"));
    }

    #[test]
    fn motion_starts_at_the_bases_own_level() {
        assert_eq!(look(&Appearance::of("alto")).unwrap().motion, "reduced");
        assert_eq!(look(&sala()).unwrap().motion, "full");
        assert_eq!(look(&Appearance { motion: Some("none".into()), ..Appearance::of("alto") }).unwrap().motion, "none");
    }

    #[test]
    fn the_resolved_appearance_fills_every_field() {
        let a = look(&Appearance::of("bancada")).unwrap().appearance;
        assert_eq!(a.ground.as_deref(), Some("bancada"));
        assert_eq!(a.accent.as_deref(), Some("#8fdcb8"));
        assert_eq!((a.text_font.as_deref(), a.heading_font.as_deref()), (Some("plex"), Some("poster")));
        assert_eq!((a.text_size, a.density, a.corners), (Some(14.5), Some(1.0), Some(1.0)));
        assert_eq!((a.motion.as_deref(), a.effects), (Some("full"), Some(ALL_EFFECTS)));
    }

    #[test]
    fn the_defaults_and_the_locked_section_agree_with_the_rest_of_the_file() {
        for id in bases() {
            let d = &token("/defaults")[id];
            assert!(choices("/user-controls/text-fonts").contains(&d["textFont"].as_str().unwrap()), "{id}");
            // A look whose heading is its own (Cobalt's Bodoni Moda) has no heading option to default to.
            assert!(d["headingFont"].as_str().is_none_or(|f| choices("/user-controls/heading-fonts").contains(&f)), "{id}");
            assert!(choices("/user-controls/motion").contains(&d["motion"].as_str().unwrap()), "{id}");
        }
        for (name, _) in MEANINGS {
            assert_eq!(token("/locked")[name]["hue"], token("/validator/accent-hue-block/near")[name], "{name}");
        }
    }

    #[test]
    fn a_legacy_appearance_falls_back_to_the_default_look() {
        let mono = json!({ "mode": "dark", "accent": "mono", "custom": null, "ui": "geistmono", "read": "geist", "density": "comfortable", "corners": "round" });
        let expected = look(&sala()).unwrap();
        assert_eq!(stored(mono.as_object()), expected);
        assert_eq!(stored(None), expected);
        let failing = colored("sala", &[("s3", "#f3f2ee")]).to_map();
        assert_eq!(stored(Some(&failing)), expected, "a saved look that fails a pair is not used");
        let oficina = Appearance::of("oficina").to_map();
        assert_eq!(stored(Some(&oficina)).appearance.base, "oficina");
    }

    #[test]
    fn the_settings_the_screen_sees_drop_an_appearance_that_cannot_validate() {
        let mono = json!({ "mode": "dark", "density": "comfortable" });
        assert_eq!(usable(mono.as_object().cloned()), None, "an old appearance is absent, so a Settings save starts from the default look");
        assert_eq!(usable(Some(colored("sala", &[("s3", "#f3f2ee")]).to_map())), None);
        assert_eq!(usable(None), None);
        let oficina = Appearance::of("oficina").to_map();
        assert_eq!(usable(Some(oficina.clone())), Some(oficina));
    }

    #[test]
    fn cobalt_has_no_heading_option_of_its_own_until_one_is_chosen() {
        assert_eq!(look(&Appearance::of("cobalto")).unwrap().appearance.heading_font, None);
        let book = Appearance { heading_font: Some("book".into()), ..Appearance::of("cobalto") };
        let look = look(&book).unwrap();
        assert_eq!(look.appearance.heading_font.as_deref(), Some("book"));
        assert_eq!(look.vars["font-display"], "\"Newsreader\",Georgia,serif");
    }

    #[test]
    fn a_frame_the_plain_inks_cannot_be_read_on_is_refused() {
        // The 26 pairs all pass: only the title bar and the sidebar, which draw on the frame, would be unreadable.
        let a = colored("sala", &[("frame", "#ffffff")]);
        let look = look(&a).unwrap();
        assert_eq!(look.checks.len(), 26);
        assert_eq!(failing_names(&look), Vec::<String>::new());
        assert!(ratio_of(&look, "ink", "frame") < 1.2);
        let refusal = validated(&a.to_map()).unwrap_err();
        assert_eq!(refusal.key, "error.look.pairFailed");
        assert_eq!(refusal.detail.as_deref(), Some("ink / frame, ink-2 / frame, ink-3 / frame"));
        for id in bases() {
            assert!(validated(&Appearance::of(id).to_map()).is_ok(), "{id}: a factory look reads on its own frame");
        }
    }

    #[test]
    fn the_text_size_moves_every_interface_size_and_keeps_each_themes_offsets() {
        let v = look(&Appearance { text_size: Some(16.0), ..Appearance::of("oficina") }).unwrap().vars;
        // Oficina draws at 14 px with a 12 px small size, a 13 px caption, a 17 px lead and an 18 px serif navigation.
        let sizes = ["fs-xs", "fs-sm", "fs-ui", "fs-read", "fs-lead", "nav-size"].map(|k| v[k].as_str());
        assert_eq!(sizes, ["14px", "15px", "16px", "17.5px", "19px", "20px"]);
        let alto = look(&Appearance { text_size: Some(15.0), ..Appearance::of("alto") }).unwrap().vars;
        assert_eq!((alto["fs-ui"].as_str(), alto["fs-xs"].as_str()), ("15px", "13px"), "Alto already draws at 15 px");
    }

    fn theme_file(extra: Value) -> Vec<u8> {
        let mut file = json!({ "format": "faber-theme", "version": 1, "name": "Mine", "base": "oficina" });
        file.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        file.to_string().into_bytes()
    }

    #[test]
    fn the_window_paints_the_looks_own_frame_first() {
        assert_eq!(frame_color(&look(&Appearance::of("oficina")).unwrap()), [0x18, 0x17, 0x15]);
        assert_eq!(frame_color(&stored(None)), [0x08, 0x08, 0x0a]);
    }

    #[test]
    fn a_theme_file_over_the_size_limit_is_refused_and_one_at_the_limit_is_read() {
        assert_eq!(parse_theme_file(&vec![b' '; 20_001]).unwrap_err().key, "error.look.fileTooBig");
        let mut at_limit = theme_file(json!({}));
        at_limit.resize(20_000, b' ');
        assert!(parse_theme_file(&at_limit).is_ok());
    }

    #[test]
    fn a_theme_file_with_anything_unknown_or_of_the_wrong_type_is_refused_with_the_field() {
        let bad = [
            (json!({ "extra": 1 }), "extra"),
            (json!({ "colors": { "s1": "var(--amber)" } }), "colors.s1"),
            (json!({ "colors": { "line": "var(--ink)" } }), "colors.line"),
            (json!({ "colors": { "amber-on": "x", "accent": "#ffffff" } }), "colors.accent"),
            (json!({ "colors": { "s1": 7 } }), "colors.s1"),
            (json!({ "colors": { "s1": "rgba(1,2,3,.5)" } }), "colors.s1"),
            (json!({ "textSize": "15" }), "textSize"),
            (json!({ "textSize": 15.2 }), "textSize"),
            (json!({ "textSize": 19 }), "textSize"),
            (json!({ "density": 0.5 }), "density"),
            (json!({ "corners": 2 }), "corners"),
            (json!({ "motion": "fast" }), "motion"),
            (json!({ "ground": "oficina2" }), "ground"),
            (json!({ "textFont": "mono" }), "textFont"),
            (json!({ "headingFont": 3 }), "headingFont"),
            (json!({ "accent": "red" }), "accent"),
            (json!({ "effects": { "ambient": "yes" } }), "effects.ambient"),
            (json!({ "effects": { "snow": true } }), "effects.snow"),
            (json!({ "base": "mono" }), "base"),
            (json!({ "format": "other" }), "format"),
            (json!({ "version": 2 }), "version"),
            (json!({ "name": "" }), "name"),
            (json!({ "name": "x".repeat(81) }), "name"),
        ];
        for (extra, field) in bad {
            let refusal = parse_theme_file(&theme_file(extra.clone())).unwrap_err();
            assert_eq!((refusal.key.as_str(), refusal.detail.as_deref()), ("error.look.fileInvalid", Some(field)), "{extra}");
        }
        assert_eq!(parse_theme_file(b"not json").unwrap_err().key, "error.look.fileInvalid");
        assert_eq!(parse_theme_file(b"[]").unwrap_err().key, "error.look.fileInvalid");
        assert!(parse_theme_file(&theme_file(json!({ "colors": { "line-2": "rgba(10, 20, 30, .5)", "s1": "#ABCDEF" } }))).is_ok());
    }

    #[test]
    fn a_theme_file_drops_the_meaning_colours_and_a_refused_accent_refuses_the_file() {
        let file = theme_file(json!({ "colors": { "amber": "#ffffff", "claude-ink": "nonsense", "codex": 1, "s2": "#202020" } }));
        let parsed = parse_theme_file(&file).unwrap();
        assert_eq!(parsed.appearance.colors.keys().collect::<Vec<_>>(), ["s2"]);
        let refusal = parse_theme_file(&theme_file(json!({ "accent": "#f2b13c" }))).unwrap_err();
        assert_eq!(refusal.key, "error.look.nearAmber");
    }

    #[test]
    fn writing_a_theme_file_and_reading_it_back_gives_the_same_appearance() {
        let a = Appearance {
            ground: Some("manha".into()),
            accent: Some("#e58fb1".into()),
            text_font: Some("atkinson".into()),
            heading_font: Some("poster".into()),
            text_size: Some(15.5),
            density: Some(0.8),
            corners: Some(0.0),
            motion: Some("reduced".into()),
            effects: Some(Effects { ambient: false, texture: true, glow: false }),
            ..colored("oficina", &[("s2", "#fbfaf6"), ("line", "rgba(28,41,36,.2)")])
        };
        let written = write_theme_file(&a.to_map(), "Workshop adjusted").unwrap();
        assert!(written.starts_with(
            "{\n  \"format\": \"faber-theme\",\n  \"version\": 1,\n  \"name\": \"Workshop adjusted\",\n  \"base\": \"oficina\""
        ));
        let read = parse_theme_file(written.as_bytes()).unwrap();
        assert_eq!((read.name.as_str(), read.appearance), ("Workshop adjusted", a));
        assert_eq!(write_theme_file(&sala().to_map(), "").unwrap_err().detail.as_deref(), Some("name"));
        let failing = colored("sala", &[("s3", "#f3f2ee")]).to_map();
        assert_eq!(write_theme_file(&failing, "Bad").unwrap_err().key, "error.look.pairFailed");
    }

    #[test]
    fn the_documented_theme_file_example_is_a_valid_file() {
        let mut example = token("/theme-file/shape").clone();
        let parsed = parse_theme_file(&serde_json::to_vec(&example).unwrap()).unwrap();
        assert!(validated(&parsed.appearance.to_map()).is_ok());
        example["extra"] = json!(true);
        assert!(parse_theme_file(&serde_json::to_vec(&example).unwrap()).is_err());
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("faber-look-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_theme_export_that_fails_halfway_leaves_the_old_file_and_no_leftover() {
        let dir = scratch("export");
        let file = dir.join("my-theme.json");
        std::fs::write(&file, "old theme").unwrap();
        // A disk that fills up after some of the bytes: the write errors out partway.
        let failed = replace_file(&file, |f| {
            f.write_all(b"half of the new")?;
            Err(io::Error::other("disk full"))
        });
        assert!(failed.is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "old theme");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temporary file is left behind");
        export_theme(&file, "new theme").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "new theme");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        // A folder where the file should go cannot be replaced: the failure is a key, not the system's sentence.
        let folder = dir.join("folder.json");
        std::fs::create_dir(&folder).unwrap();
        assert_eq!(export_theme(&folder, "x").unwrap_err(), Failure::key("error.theme.write"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_theme_file_that_cannot_be_read_fails_with_a_key() {
        let dir = scratch("import");
        assert_eq!(import_theme(&dir.join("missing.json")).unwrap_err(), Failure::key("error.theme.read"));
        let file = dir.join("ok.json");
        export_theme(&file, &write_theme_file(&sala().to_map(), "Mine").unwrap()).unwrap();
        assert_eq!(import_theme(&file).unwrap().name, "Mine");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
