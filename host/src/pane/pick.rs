//! Pick mode (task 205): the person points at something on the page in front of them and asks about it. `act.js` lays a
//! clear layer over the page that outlines what is under the pointer and keeps every click from the page; here the screen
//! waits for the person's pick a few seconds at a time and gets it as words (role, name, visible text, a selector), each
//! passed through [`mask`], with a small picture of it when nothing secret is in it.
//!
//! Nothing runs while the mode is off, and nothing is kept: the layer lives in the page only while the screen waits on it,
//! and goes by itself a few seconds after the screen stops asking.

use super::Pane;
use super::rules::act_function;
use crate::Res;
use base64::Engine as _;
use faber_engine::browser::mask::mask;
use serde_json::{Value, json};
use std::sync::Arc;

/// How long one wait for the person's pick lasts in the page: well under the pane's own deadline for a call (`CALL`).
const PICK_WAIT_MS: u64 = 5_000;
/// The most of the page's words a pick brings, in characters.
const MAX_PICK_TEXT: usize = 200;
const MAX_PICK_NAME: usize = 80;
const MAX_PICK_SELECTOR: usize = 300;
const MAX_PICK_PAGE: usize = 300;
/// The largest side of a pick's picture, in page pixels: a picked section never becomes a whole-page picture.
const MAX_PICK_SIDE: f64 = 1_200.0;

impl Pane {
    /// Pick mode on conversation `id`'s page in front of the person. With `on`, waits up to [`PICK_WAIT_MS`] for a pick and
    /// answers `{ picked, picture }`, `{ waiting: true }` (ask again) or `{ ended: true }` (Esc, the page went, or it is not
    /// this conversation's page in view). Without `on`, ends the mode.
    pub async fn site_pick(self: &Arc<Self>, id: String, on: bool) -> Res<Value> {
        let ended = json!({ "ended": true });
        let Some(before) = self.pick_view(&id, on) else { return Ok(ended) };
        // A page that went to another document took the world, and the layer, with it.
        let Ok(context) = self.world().await else { return Ok(ended) };
        let request = json!({ "kind": if on { "pick" } else { "unpick" }, "wait": PICK_WAIT_MS, "mark": self.mark });
        let expression = format!("({})({request})", act_function());
        let params = json!({ "expression": expression, "returnByValue": true, "awaitPromise": true, "contextId": context });
        let Ok(answer) = self.call("Runtime.evaluate", params).await else { return Ok(ended) };
        let value = &answer["result"]["value"];
        if !on || value["ended"].as_bool() == Some(true) || !value.is_object() {
            return Ok(ended);
        }
        if value["waiting"].as_bool() == Some(true) {
            return Ok(json!({ "waiting": true }));
        }
        let picked = &value["picked"];
        if !picked.is_object() {
            return Ok(ended);
        }
        self.touch();
        // Masked whole, then cut: a key that starts before the cut is hidden whole, never left half readable.
        let full = |key: &str| picked[key].as_str().unwrap_or_default().chars().filter(|c| !c.is_control()).collect::<String>();
        let words = |key: &str, max: usize| -> (String, bool) {
            let raw = full(key);
            let masked = mask(&raw);
            let changed = masked != raw;
            (masked.chars().take(max).collect(), changed)
        };
        let (role, _) = words("role", 40);
        let (name, name_masked) = words("name", MAX_PICK_NAME);
        let (text, text_masked) = words("text", MAX_PICK_TEXT);
        let (selector, selector_masked) = words("selector", MAX_PICK_SELECTOR);
        // A key in the page's words, or in a text field the picture would reach, would show in the picture: none then.
        let near_masked = picked["near"].as_array().into_iter().flatten().filter_map(Value::as_str).any(|v| mask(v) != v);
        let quiet = picked["secret"].as_bool() != Some(false)
            || value["turn"].as_str() == Some("a payment")
            || name_masked
            || text_masked
            || selector_masked
            || near_masked;
        let href = full("href");
        // The picture is of this pick's view and document only: a tab, page or view that changed meanwhile gets none.
        let same = |pane: &Self| pane.pick_view(&id, true).is_some_and(|now| now == before && same_document(now.url.as_deref(), &href));
        let mut picture = None;
        if !quiet && same(self) {
            picture = self.pick_picture(picked).await;
            if !same(self) {
                picture = None;
            }
        }
        Ok(json!({
            "picked": { "role": role, "name": name, "text": text, "selector": selector, "page": page_words(&href) },
            "picture": picture.map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes)),
        }))
    }

    /// What says which page a pick is on: the pane, its conversation and tab, the address the view last reported and how many
    /// pages started in it. None when it is not conversation `id`'s page (in view, for `on`).
    fn pick_view(&self, id: &str, on: bool) -> Option<PickView> {
        let s = crate::lock(&self.state);
        if !s.open || s.owner.as_deref() != Some(id) || (on && !s.shown) {
            return None;
        }
        Some(PickView { made: s.made, tab: s.tab.clone(), url: s.url.clone(), started: self.loads.borrow().0 })
    }

    /// A JPEG of the picked element's own box, cut from the page as the agent's picture of one control is (`shoot`), none when
    /// it has no size.
    async fn pick_picture(&self, picked: &Value) -> Option<Vec<u8>> {
        self.capture(pick_clip(picked)?).await
    }
}

/// The page area a pick's picture covers: the part of the element's box on screen, in page coordinates.
fn pick_clip(picked: &Value) -> Option<Value> {
    let edge = |i: usize| picked["rect"][i].as_f64().filter(|v| v.is_finite()).unwrap_or(0.0);
    let scroll = |key: &str| picked[key].as_f64().filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0).min(1e7);
    if edge(2) < 1.0 || edge(3) < 1.0 {
        return None;
    }
    // Only the part of the box on screen: a box starting off screen is cut, never shifted onto its neighbours.
    let (x0, y0) = (edge(0).max(0.0), edge(1).max(0.0));
    let view = |key: &str| picked[key].as_f64().filter(|v| v.is_finite() && *v > 0.0).unwrap_or(0.0);
    let (x1, y1) = ((edge(0) + edge(2)).min(view("vw")), (edge(1) + edge(3)).min(view("vh")));
    let (w, h) = ((x1 - x0).min(MAX_PICK_SIDE), (y1 - y0).min(MAX_PICK_SIDE));
    if w < 1.0 || h < 1.0 {
        return None;
    }
    let (left, top) = (scroll("x") + x0, scroll("y") + y0);
    Some(json!({ "x": left, "y": top, "width": w, "height": h, "scale": if w.max(h) <= 400.0 { 2.0 } else { 1.0 } }))
}

/// Which page a pick is on (`Pane::pick_view`).
#[derive(Debug, PartialEq)]
struct PickView {
    made: u64,
    tab: Option<String>,
    url: Option<String>,
    started: u64,
}

/// Whether the address the view reported and the document's own are the same page (a fragment aside).
fn same_document(reported: Option<&str>, href: &str) -> bool {
    let page = |url: &str| url.split('#').next().unwrap_or_default().to_string();
    reported.is_some_and(|reported| page(reported) == page(href))
}

/// The page's address as a pick names it: its origin and path only (a query or a fragment can hold a token), masked, and at
/// most [`MAX_PICK_PAGE`] characters.
fn page_words(url: &str) -> String {
    let Ok(parsed) = url::Url::parse(url) else { return String::new() };
    if !matches!(parsed.scheme(), "http" | "https") {
        return String::new();
    }
    let origin = parsed.origin().ascii_serialization();
    // A path that hides a token behind percent escapes is judged as decoded; one that holds a secret is left out whole.
    if mask(&decoded(parsed.path())) != decoded(parsed.path()) {
        return mask(&origin).chars().take(MAX_PICK_PAGE).collect();
    }
    mask(&format!("{origin}{}", parsed.path())).chars().take(MAX_PICK_PAGE).collect()
}

/// A path with its `%XX` escapes turned back into the characters they stand for.
fn decoded(path: &str) -> String {
    let bytes = path.as_bytes();
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
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{page_words, pick_clip};
    use serde_json::json;

    #[test]
    fn a_box_starting_off_screen_is_cut_never_shifted_onto_its_neighbours() {
        let clip = pick_clip(&json!({ "rect": [-100, 0, 200, 40], "x": 0, "y": 30, "vw": 800, "vh": 600 })).unwrap();
        assert_eq!((clip["x"].as_f64(), clip["y"].as_f64(), clip["width"].as_f64()), (Some(0.0), Some(30.0), Some(100.0)));
        let right = pick_clip(&json!({ "rect": [780, 590, 100, 40], "x": 0, "y": 0, "vw": 800, "vh": 600 })).unwrap();
        assert_eq!((right["width"].as_f64(), right["height"].as_f64()), (Some(20.0), Some(10.0)));
        assert!(pick_clip(&json!({ "rect": [-300, 0, 200, 40], "x": 0, "y": 0, "vw": 800, "vh": 600 })).is_none());
        assert!(pick_clip(&json!({ "rect": [0, 0, 200, 40], "x": 0, "y": 0 })).is_none(), "no viewport size, no picture");
    }

    #[test]
    fn a_picked_page_is_named_without_its_sign_in_part_query_or_fragment() {
        assert_eq!(page_words("https://me:pw@app.example/settings?token=abc#top"), "https://app.example/settings");
        let hidden = "https://app.example/reset/%67%68%70%5F%61%42%33%63%44%34%65%46%35%67%48%36%69%4A%37%6B%4C%38%6D%4E%39%6F";
        assert_eq!(page_words(hidden), "https://app.example");
        assert_eq!(page_words("http://localhost:5173/reset/ghp_16C7e42F292c6912E7710c838347Ae178B4a"), "http://localhost:5173");
        assert_eq!(page_words("not an address"), "");
        assert_eq!(page_words("file:///C:/secret"), "");
        assert_eq!(page_words(&format!("https://app.example/{}", "a".repeat(100_000))).chars().count(), 300, "a long path is cut");
    }
}
