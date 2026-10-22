//! A conversation's tabs in Faber's browser (R431): where each one is, its title, which one is in front and which one the
//! agent drives. They live in the conversation's file, so closing the pane, its idle close, another conversation and a
//! restart all leave them there; the pane loads the front one again. Never browser storage, never an export, never a
//! picture of a page (R236), and nothing of a site's own data: cookies and sign-ins stay in the pane and end at quit
//! (R308, R370).

use super::{MAX_TITLE, MAX_URL, checked_url};
use serde::{Deserialize, Serialize};

/// The most tabs a conversation keeps; the oldest one that is neither in front nor the agent's goes first.
pub const MAX_TABS: usize = 16;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tabs {
    /// In the order the strip shows them.
    pub tabs: Vec<Tab>,
    /// The tab in front, by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub front: Option<String>,
    /// The tab the agent opens its pages in and acts on, by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// The number the next tab's id takes, so an id is never given twice, even after the highest tab closed.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    /// `t1`, `t2`…: only grows within a conversation, so a tab closed and a new one never share a name.
    pub id: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
}

/// The address as a tab keeps it: a web page the pane may load, at most [`MAX_URL`] long, without its fragment (where
/// sign-ins put one-time tokens, `#access_token=`). None for anything else.
pub fn kept_url(url: &str) -> Option<String> {
    let url = checked_url(&serde_json::json!({ "url": url })).ok()?;
    let url = url.split_once('#').map_or(url.as_str(), |(page, _)| page);
    (url.chars().count() <= MAX_URL).then(|| url.to_string())
}

/// Characters that change how text around them shows without showing themselves: bidirectional overrides and isolates,
/// zero-width spaces and joiners, the byte order mark. A page's title never carries them into Faber's head.
fn hidden(c: char) -> bool {
    matches!(c, '\u{ad}' | '\u{61c}' | '\u{180e}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}')
}

/// The title as a tab keeps it: one line of at most [`MAX_TITLE`] characters.
pub fn kept_title(title: &str) -> String {
    // A huge title is cut before it is cleaned, so a page cannot make Faber copy megabytes.
    let line: String = title.chars().take(MAX_TITLE * 4).filter(|c| !hidden(*c)).map(|c| if c.is_control() { ' ' } else { c }).collect();
    line.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_TITLE).collect()
}

impl Tabs {
    pub fn get(&self, id: &str) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }

    /// The tab in front, when there is one.
    pub fn front_tab(&self) -> Option<&Tab> {
        self.front.as_deref().and_then(|id| self.get(id))
    }

    /// A new tab's id: one more than any this conversation gave (a file from before the counter counts its tabs).
    fn next_id(&mut self) -> String {
        let top = self.tabs.iter().filter_map(|t| t.id.strip_prefix('t')?.parse::<u64>().ok()).max().unwrap_or(0);
        self.next = self.next.max(top) + 1;
        format!("t{}", self.next)
    }

    /// Adds a tab at the end of the strip and answers its id. Past [`MAX_TABS`] the oldest that is neither in front nor the
    /// agent's goes.
    pub fn add(&mut self, url: String, title: String) -> String {
        let id = self.next_id();
        self.tabs.push(Tab { id: id.clone(), url, title });
        while self.tabs.len() > MAX_TABS {
            let Some(at) = self.tabs.iter().position(|t| Some(&t.id) != self.front.as_ref() && Some(&t.id) != self.agent.as_ref()) else {
                break;
            };
            self.tabs.remove(at);
        }
        id
    }

    /// A page finished loading in tab `tab` (none: the one in front, made if there is none), with its title when it is
    /// known. A new address forgets the old title. Answers whether anything changed, so an unchanged load writes nothing.
    pub fn record(&mut self, tab: Option<&str>, url: &str, title: Option<&str>) -> bool {
        let Some(url) = kept_url(url) else { return false };
        let title = title.map(kept_title);
        let id = match tab.or(self.front.as_deref()).filter(|id| self.get(id).is_some()) {
            Some(id) => id.to_string(),
            None if tab.is_some() => return false,
            None => {
                let id = self.add(url.clone(), title.clone().unwrap_or_default());
                self.front = Some(id);
                return true;
            }
        };
        let Some(t) = self.tabs.iter_mut().find(|t| t.id == id) else { return false };
        let before = t.clone();
        if t.url != url {
            t.url = url;
            t.title.clear();
        }
        if let Some(title) = title {
            t.title = title;
        }
        *t != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_load_makes_the_front_tab_and_a_new_address_forgets_the_old_title() {
        let mut tabs = Tabs::default();
        assert!(tabs.record(None, "https://example.org/a", Some("A")));
        assert_eq!(tabs.front.as_deref(), Some("t1"));
        assert_eq!(tabs.front_tab().map(|t| (t.url.as_str(), t.title.as_str())), Some(("https://example.org/a", "A")));
        assert!(!tabs.record(None, "https://example.org/a", Some("A")), "the same page writes nothing");
        assert!(tabs.record(None, "https://example.org/b", None));
        assert_eq!(tabs.front_tab().map(|t| t.title.as_str()), Some(""));
        assert_eq!(tabs.tabs.len(), 1, "one tab until the person opens another");
    }

    #[test]
    fn only_web_pages_are_kept_and_titles_are_one_short_line() {
        let mut tabs = Tabs::default();
        for refused in ["file:///C:/x", "javascript:alert(1)", "https://tauri.localhost/", "https://me:pw@example.org/", "about:blank"] {
            assert!(!tabs.record(None, refused, None), "{refused}");
        }
        assert!(tabs.tabs.is_empty());
        assert!(!tabs.record(None, &format!("https://example.org/{}", "a".repeat(MAX_URL)), None), "too long");
        tabs.record(None, "http://localhost:3000/", Some(&format!("Line\none\t{}", "x".repeat(MAX_TITLE))));
        let title = &tabs.front_tab().unwrap().title;
        assert!(title.starts_with("Line one x") && title.chars().count() == MAX_TITLE, "{title}");
        assert_eq!(kept_title("Safe\u{202e}txt.exe\u{200b} page"), "Safetxt.exe page", "no character that hides or turns text");
        assert_eq!(kept_url("https://example.org/back#access_token=x").as_deref(), Some("https://example.org/back"), "no fragment");
    }

    #[test]
    fn tab_ids_only_grow_and_the_front_and_agent_tabs_outlive_the_cap() {
        let mut tabs = Tabs::default();
        tabs.record(None, "https://example.org/0", None);
        tabs.agent = Some(tabs.add("https://example.org/agent".into(), String::new()));
        for n in 0..MAX_TABS + 3 {
            tabs.add(format!("https://example.org/{n}"), String::new());
        }
        assert_eq!(tabs.tabs.len(), MAX_TABS);
        assert!(tabs.get("t1").is_some() && tabs.get("t2").is_some(), "the front and the agent's tabs stay");
        assert_eq!(tabs.add("https://example.org/next".into(), String::new()), format!("t{}", MAX_TABS + 6));
        assert!(!tabs.record(Some("t3"), "https://example.org/gone", None), "a tab that went records nothing");
        tabs.tabs.pop();
        assert_eq!(tabs.add(String::new(), String::new()), format!("t{}", MAX_TABS + 7), "the highest gone, its id is not given again");
    }

    #[test]
    fn an_old_file_without_tabs_and_a_saved_record_read_back_the_same() {
        let mut tabs = Tabs::default();
        tabs.record(None, "https://example.org/", Some("Example"));
        let json = serde_json::to_value(&tabs).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "tabs": [{ "id": "t1", "url": "https://example.org/", "title": "Example" }], "front": "t1", "next": 1 })
        );
        let old: Tabs = serde_json::from_value(serde_json::json!({ "tabs": [{ "id": "t4", "url": "https://example.org/" }] })).unwrap();
        assert_eq!(old.clone().add(String::new(), String::new()), "t5", "a file from before the counter goes on from its tabs");
        assert_eq!(serde_json::from_value::<Tabs>(json).unwrap(), tabs);
    }
}
