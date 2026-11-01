//! The page, as a scenario sees it: elements found by role, label, text or CSS, and things a person does to them.
//! Everything here goes through `cdp`; the finding itself is `locator.js`, which runs inside the page.

use crate::cdp::{Accept, Cdp, DEFAULT_WAIT, Settled};
use serde_json::{Value, json};
use std::path::Path;
use std::time::{Duration, Instant};

const LOCATOR: &str = include_str!("locator.js");

/// Where to look, as steps that narrow the matches one after another. Build one with [`css`], [`role`], [`label`],
/// [`text`] or [`text_re`], and add steps with the methods.
#[derive(Debug, Clone)]
pub struct Loc {
    steps: Vec<Value>,
}

pub fn css(selector: &str) -> Loc {
    Loc { steps: vec![] }.css(selector)
}

/// An element with this role whose name contains `name` (any case). Call `.exact()` for the whole name.
pub fn role(role: &str, name: &str) -> Loc {
    Loc { steps: vec![] }.role(role, name)
}

/// An element with this role, whatever its name.
pub fn role_any(role: &str) -> Loc {
    Loc { steps: vec![json!({ "k": "role", "role": role })] }
}

/// A control labelled with this text (its `aria-label` or its `<label>`).
pub fn label(text: &str) -> Loc {
    Loc { steps: vec![json!({ "k": "label", "want": { "text": text }, "exact": false })] }
}

pub fn text(text: &str) -> Loc {
    Loc { steps: vec![] }.text(text)
}

impl Loc {
    pub fn css(mut self, selector: &str) -> Loc {
        self.steps.push(json!({ "k": "css", "sel": selector }));
        self
    }

    pub fn role(mut self, role: &str, name: &str) -> Loc {
        self.steps.push(json!({ "k": "role", "role": role, "want": { "text": name }, "exact": false }));
        self
    }

    /// The smallest elements whose text contains this text (any case). Call `.exact()` for the whole text.
    pub fn text(mut self, text: &str) -> Loc {
        self.steps.push(json!({ "k": "text", "want": { "text": text }, "exact": false }));
        self
    }

    /// The smallest elements whose text matches this JavaScript regular expression.
    pub fn text_re(mut self, pattern: &str) -> Loc {
        self.steps.push(json!({ "k": "text", "want": { "re": pattern, "flags": "" }, "exact": false }));
        self
    }

    /// Keeps only matches whose text contains this text (any case).
    pub fn has_text(mut self, text: &str) -> Loc {
        self.steps.push(json!({ "k": "has", "text": text }));
        self
    }

    pub fn last(mut self) -> Loc {
        self.steps.push(json!({ "k": "last" }));
        self
    }

    /// Makes the last role, label or text step match the whole name or text, in the same case.
    pub fn exact(mut self) -> Loc {
        if let Some(step) = self.steps.last_mut() {
            step["exact"] = json!(true);
        }
        self
    }

    fn steps_json(&self) -> String {
        Value::Array(self.steps.clone()).to_string()
    }

    fn is_named_button(&self) -> bool {
        self.steps.len() == 1
            && self.steps[0]["k"] == "role"
            && self.steps[0]["role"] == "button"
            && self.steps[0]["want"]["text"].is_string()
    }

    /// A line for a failure message.
    pub fn describe(&self) -> String {
        let parts: Vec<String> = self
            .steps
            .iter()
            .map(|s| match s["k"].as_str().unwrap_or("?") {
                "css" => format!("css {}", s["sel"]),
                "role" => format!("role {} {}", s["role"], s["want"]["text"]),
                "label" => format!("label {}", s["want"]["text"]),
                "text" => format!("text {}", if s["want"]["re"].is_null() { s["want"]["text"].clone() } else { s["want"]["re"].clone() }),
                "has" => format!("has text {}", s["text"]),
                other => other.to_string(),
            })
            .collect();
        parts.join(" > ")
    }
}

pub struct Page {
    pub cdp: Cdp,
}

impl Page {
    pub fn connect(port: u16, timeout: Duration) -> Result<Page, String> {
        Ok(Page { cdp: Cdp::connect(port, timeout)? })
    }

    pub fn eval(&mut self, expression: &str) -> Result<Value, String> {
        self.cdp.eval(expression)
    }

    pub fn reload(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + DEFAULT_WAIT;
        self.cdp.reload(deadline)?;
        self.wait_ready(deadline)
    }

    pub(crate) fn wait_ready(&mut self, deadline: Instant) -> Result<(), String> {
        self.wait_for_within(
            "the screen and its test helpers to be ready",
            "!!document.querySelector('.app') && typeof window.__faberText === 'function' && typeof window.__faberMojibake === 'function'",
            deadline.saturating_duration_since(Instant::now()),
        )
    }

    pub fn wait_for(&mut self, what: &str, expression: &str) -> Result<(), String> {
        self.cdp.wait_for(what, expression, DEFAULT_WAIT)
    }

    pub fn wait_for_within(&mut self, what: &str, expression: &str, timeout: Duration) -> Result<(), String> {
        self.cdp.wait_for(what, expression, timeout)
    }

    pub fn screenshot(&mut self, path: &Path) -> Result<(), String> {
        self.cdp.screenshot(path)
    }

    pub fn press(&mut self, chord: &str) -> Result<(), String> {
        self.cdp.press(chord)
    }

    // The words on screen. The screen exposes its own translations in test builds, so scenarios never write a sentence.

    /// The text for `key` in the language the screen shows now.
    pub fn text_of(&mut self, key: &str) -> Result<String, String> {
        self.text_with(key, &json!({}), None)
    }

    /// The text for `key` in a given language (`en`, `pt-BR`, `es`, `fr`, `de`), whatever the screen shows.
    pub fn text_in(&mut self, key: &str, language: &str) -> Result<String, String> {
        self.text_with(key, &json!({}), Some(language))
    }

    pub fn text_with(&mut self, key: &str, vars: &Value, language: Option<&str>) -> Result<String, String> {
        let call = format!("window.__faberText({}, {}, {})", json!(key), vars, json!(language));
        let value = self.cdp.eval(&format!("(typeof window.__faberText === 'function') ? {call} : null"))?;
        value
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| format!("window.__faberText is missing or has no text for {key}: is this a test build?"))
    }

    /// A role locator named by a translation key: `named("heading", "empty.chatTitle")`.
    pub fn named(&mut self, role: &str, key: &str) -> Result<Loc, String> {
        Ok(self::role(role, &self.text_of(key)?))
    }

    pub fn named_in(&mut self, role: &str, key: &str, language: &str) -> Result<Loc, String> {
        Ok(self::role(role, &self.text_in(key, language)?))
    }

    /// A control labelled by a translation key.
    pub fn labelled(&mut self, key: &str) -> Result<Loc, String> {
        // The whole name: "Edit message" must not answer to "Message".
        Ok(label(&self.text_of(key)?).exact())
    }

    // Looking.

    fn probe(&mut self, loc: &Loc, body: &str) -> Result<Value, String> {
        self.cdp.eval(&format!("(() => {{ {LOCATOR}\n const steps = {}; {body} }})()", loc.steps_json()))
    }

    pub fn count(&mut self, loc: &Loc) -> Result<usize, String> {
        Ok(self.probe(loc, "return window.__tk.find(steps).length;")?.as_u64().unwrap_or(0) as usize)
    }

    pub fn is_visible(&mut self, loc: &Loc) -> Result<bool, String> {
        Ok(self.probe(loc, "return window.__tk.find(steps).some(window.__tk.shown);")?.as_bool().unwrap_or(false))
    }

    pub fn wait_visible(&mut self, loc: &Loc) -> Result<(), String> {
        self.wait_until(&format!("{} to be visible", loc.describe()), loc, "window.__tk.find(steps).some(window.__tk.shown)", DEFAULT_WAIT)
    }

    pub fn wait_gone(&mut self, loc: &Loc) -> Result<(), String> {
        self.wait_until(&format!("{} to disappear", loc.describe()), loc, "!window.__tk.find(steps).some(window.__tk.shown)", DEFAULT_WAIT)
            .map_err(|e| self.button_diagnostic(loc, e))
    }

    fn button_diagnostic(&mut self, loc: &Loc, error: String) -> String {
        if !loc.is_named_button() {
            return error;
        }
        let details = self.probe(loc, r#"
            return window.__tk.find(steps).filter(window.__tk.shown).map((el) => {
                const ancestors = [];
                for (let p = el.parentElement; p && ancestors.length < 3; p = p.parentElement)
                    ancestors.push(p.tagName.toLowerCase() + (p.getAttribute('class') ? '.' + p.getAttribute('class').trim().replace(/\s+/g, '.') : ''));
                const context = el.parentElement?.parentElement ?? el.parentElement;
                const text = (context?.innerText ?? '').replace(/\s+/g, ' ').trim().slice(0, 60);
                return { ancestors, text };
            });
        "#);
        match details {
            Ok(value) => format!("{error}\nVisible matching buttons (nearest ancestors first): {value}"),
            Err(e) => format!("{error}\nCould not inspect matching buttons: {e}"),
        }
    }

    pub fn wait_count(&mut self, loc: &Loc, count: usize) -> Result<(), String> {
        let what = format!("{} to match {count} elements", loc.describe());
        self.wait_until(&what, loc, &format!("window.__tk.find(steps).length === {count}"), DEFAULT_WAIT)
    }

    fn wait_until(&mut self, what: &str, loc: &Loc, condition: &str, timeout: Duration) -> Result<(), String> {
        let expression = format!("(() => {{ {LOCATOR}\n const steps = {}; return {condition}; }})()", loc.steps_json());
        self.cdp.wait_for(what, &expression, timeout)
    }

    // Reading and acting on the one element a locator names. Each waits (up to 15 s) until there is exactly one, and
    // that it is visible and enabled when the action needs it; a locator that matches several is a mistake in the scenario.

    fn on(&mut self, loc: &Loc, need: &str, body: &str) -> Result<Value, String> {
        let script = format!(
            "(() => {{ {LOCATOR}\n const found = window.__tk.one({}, '{need}'); if (!found.el) return {{ why: found.why }}; const el = found.el; {body} }})()",
            loc.steps_json()
        );
        let started = Instant::now();
        let deadline = started + DEFAULT_WAIT;
        let outer = self.cdp.set_deadline(Some(deadline));
        let settled = self.cdp.settle(&script, Accept::NoWhy, deadline);
        self.cdp.set_deadline(outer);
        let result = match settled {
            Ok(Settled::Accepted(value)) => Ok(value),
            Ok(Settled::TimedOut(value)) => {
                let why = value["why"].as_str().unwrap_or("not ready");
                let error = format!("{}: {why}", loc.describe());
                Err(if why.contains("elements match, but one was expected") { self.button_diagnostic(loc, error) } else { error })
            }
            Err(e) => Err(e),
        };
        crate::timing::step("page-act", started.elapsed());
        result
    }

    // A pointer action waits for finite layout motion, just as it already waits for visibility and enabled state.
    const CENTER: &'static str = "if (document.documentElement.hasAttribute('data-theme-transition')) return { why: 'theme snapshot is changing' }; \
        el.scrollIntoView({ block: 'center', inline: 'center' }); \
        for (let p = el; p; p = p.parentElement) { \
            if (p.getAnimations().some((a) => a.playState === 'running' && Number.isFinite(a.effect.getTiming().iterations) && \
                a.effect.getKeyframes().some((f) => ['transform', 'width', 'height', 'paddingRight', 'gridTemplateColumns', 'gridTemplateRows'].some((key) => key in f)))) \
                return { why: 'element is moving' }; \
        } \
        const r = el.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 };";

    pub fn inner_text(&mut self, loc: &Loc) -> Result<String, String> {
        Ok(self.on(loc, "attached", "return { value: el.innerText };")?["value"].as_str().unwrap_or_default().to_string())
    }

    pub fn attr(&mut self, loc: &Loc, name: &str) -> Result<Option<String>, String> {
        let value = self.on(loc, "attached", &format!("return {{ value: el.getAttribute({}) }};", json!(name)))?;
        Ok(value["value"].as_str().map(str::to_string))
    }

    pub fn is_disabled(&mut self, loc: &Loc) -> Result<bool, String> {
        Ok(self.on(loc, "attached", "return { value: !!el.disabled };")?["value"].as_bool().unwrap_or(false))
    }

    pub fn is_checked(&mut self, loc: &Loc) -> Result<bool, String> {
        Ok(self.on(loc, "attached", "return { value: !!el.checked };")?["value"].as_bool().unwrap_or(false))
    }

    /// Runs `body` with the element as `el`. It must `return` an object, which is handed back.
    pub fn eval_on(&mut self, loc: &Loc, body: &str) -> Result<Value, String> {
        self.on(loc, "attached", body)
    }

    pub fn click(&mut self, loc: &Loc) -> Result<(), String> {
        // Visibility can precede the rendered layout after navigation or scrolling.
        let stable = "el.scrollIntoView({ block: 'center', inline: 'center' }); const before = el.getBoundingClientRect(); return new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => { const r = el.getBoundingClientRect(); const x = r.x + r.width / 2, y = r.y + r.height / 2; const hit = document.elementFromPoint(x, y); resolve(el.isConnected && r.x === before.x && r.y === before.y && r.width === before.width && r.height === before.height && hit && el.contains(hit) ? { x, y } : { why: 'the click target has not reached its rendered layout' }); })));";
        let script = format!("const at = (() => {{ {} }})(); if (at.why) return at; ", Self::CENTER) + stable;
        let at = self.on(loc, "enabled", &script)?;
        self.cdp.move_to(at["x"].as_f64().unwrap_or_default(), at["y"].as_f64().unwrap_or_default())?;
        // Pointer entry can resume offscreen motion or change hover layout; settle it before the one button press.
        self.on(loc, "enabled", Self::CENTER)?;
        let at = self.on(loc, "enabled", stable)?;
        self.cdp.click_at(at["x"].as_f64().unwrap_or_default(), at["y"].as_f64().unwrap_or_default())
    }

    /// Presses the right mouse button on the middle of the element.
    pub fn right_click(&mut self, loc: &Loc) -> Result<(), String> {
        let at = self.on(loc, "visible", &format!("el.scrollIntoView({{ block: 'center', inline: 'center' }}); {}", Self::CENTER))?;
        self.cdp.right_click_at(at["x"].as_f64().unwrap_or_default(), at["y"].as_f64().unwrap_or_default())
    }

    /// Waits until the element is visible and not disabled, without acting on it.
    pub fn wait_enabled(&mut self, loc: &Loc) -> Result<(), String> {
        self.on(loc, "enabled", "return {};").map(drop)
    }

    pub fn hover(&mut self, loc: &Loc) -> Result<(), String> {
        let at = self.on(loc, "visible", Self::CENTER)?;
        self.cdp.move_to(at["x"].as_f64().unwrap_or_default(), at["y"].as_f64().unwrap_or_default())
    }

    /// Replaces what is in a text field with `text`, typed the way the keyboard would.
    pub fn fill(&mut self, loc: &Loc, text: &str) -> Result<(), String> {
        self.on(loc, "enabled", "el.focus(); if (el.select) el.select(); return {};")?;
        self.cdp.insert_text(text)
    }

    /// Focuses the element, then presses a key or chord (`Enter`, `Control+,`).
    pub fn press_on(&mut self, loc: &Loc, chord: &str) -> Result<(), String> {
        self.on(loc, "visible", "el.focus(); return {};")?;
        self.cdp.press(chord)
    }

    /// Picks the option of a `<select>` whose value or text is `choice`.
    pub fn select_option(&mut self, loc: &Loc, choice: &str) -> Result<(), String> {
        let body = format!(
            "const o = [...el.options].find((o) => o.value === {c} || o.text === {c}); if (!o) return {{ why: 'no option ' + {c} }}; \
             el.value = o.value; el.dispatchEvent(new Event('input', {{ bubbles: true }})); el.dispatchEvent(new Event('change', {{ bubbles: true }})); return {{}};",
            c = json!(choice)
        );
        self.on(loc, "enabled", &body).map(drop)
    }

    /// Ticks a checkbox that is not ticked yet.
    pub fn check(&mut self, loc: &Loc) -> Result<(), String> {
        let state = self.on(
            loc,
            "enabled",
            &format!(
                "const at = (() => {{ {} }})(); if (at.why) return at; return {{ checked: el.checked, x: at.x, y: at.y }};",
                Self::CENTER
            ),
        )?;
        if state["checked"] == json!(true) {
            return Ok(());
        }
        self.cdp.click_at(state["x"].as_f64().unwrap_or_default(), state["y"].as_f64().unwrap_or_default())?;
        if self.is_checked(loc)? { Ok(()) } else { Err(format!("{}: still not checked after the click", loc.describe())) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locators_are_built_step_by_step_and_exact_applies_to_the_last_step() {
        let loc = role("dialog", "Connect your AI").exact().role("button", "Done").exact();
        assert_eq!(loc.steps[0]["exact"], json!(true));
        assert_eq!(loc.steps[1]["exact"], json!(true));
        let loc = role("button", "Settings").css(".x").last();
        assert_eq!(loc.steps[0]["exact"], json!(false));
        assert_eq!(loc.steps.last().unwrap()["k"], "last");
        assert_eq!(css(".list .item").has_text("Long one").steps[1], json!({ "k": "has", "text": "Long one" }));
        assert_eq!(text("Connected").exact().steps[0]["want"]["text"], "Connected");
        assert_eq!(text("x").text_re("Ray(leigh)").steps[1]["want"], json!({ "re": "Ray(leigh)", "flags": "" }));
        assert!(role_any("checkbox").steps[0].get("want").is_none(), "any name");
    }

    #[test]
    fn a_locator_describes_itself_for_failure_messages() {
        let loc = css(".setup-card[data-vendor=\"codex\"]").text("Connected").exact().last();
        assert_eq!(loc.describe(), "css \".setup-card[data-vendor=\\\"codex\\\"]\" > text \"Connected\" > last");
        assert_eq!(label("Message").describe(), "label \"Message\"");
    }

    #[test]
    fn only_named_button_locators_get_button_diagnostics() {
        assert!(role("button", "Stop").is_named_button());
        assert!(!role_any("button").is_named_button());
        assert!(!role("heading", "Stop").is_named_button());
        assert!(!css(".composer").role("button", "Stop").is_named_button());
    }

    #[test]
    fn the_locator_script_installs_itself_once_and_exposes_what_the_page_calls_need() {
        for needed in ["window.__tk = { find, one, shown }", "if (window.__tk) return;", "unknown locator step"] {
            assert!(LOCATOR.contains(needed), "{needed}");
        }
    }
}
