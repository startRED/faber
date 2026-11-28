//! A conversation's tabs in Faber's browser (R431): where each one is, its title, which one is in front and which one the
//! agent drives. They live in the conversation's file, so closing the pane, its idle close, another conversation and a
//! restart all leave them there; the pane loads the front one again. Never browser storage, never an export, never a
//! picture of a page (R236), and nothing of a site's own data: cookies and sign-ins stay in the pane and end at quit
//! (R308, R370).

use super::{MAX_TITLE, MAX_URL, checked_url};
use serde::{Deserialize, Serialize};

/// The most tabs a conversation keeps; the oldest one that is neither in front nor the agent's goes first.
pub const MAX_TABS: usize = 16;
/// The most closed tabs kept for "Reopen closed tab", the latest first out.
pub const MAX_CLOSED: usize = 8;

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
    /// Tabs the person closed, oldest first, for "Reopen closed tab".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub closed: Vec<Tab>,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tab {
    /// `t1`, `t2`…: only grows within a conversation, so a tab closed and a new one never share a name.
    pub id: String,
    /// Empty for a new tab that has opened nothing yet.
    #[serde(default)]
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

    /// A new blank tab at the end of the strip, brought to the front. A blank tab already in front is kept instead.
    pub fn new_tab(&mut self) -> String {
        if let Some(front) = self.front_tab().filter(|t| t.url.is_empty()) {
            return front.id.clone();
        }
        let id = self.add(String::new(), String::new());
        self.front = Some(id.clone());
        id
    }

    /// Brings tab `id` to the front. False when there is no such tab.
    pub fn bring(&mut self, id: &str) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        self.front = Some(id.to_string());
        true
    }

    /// Closes tab `id`; the one after it comes to the front (the one before it when it was the last), and a tab that had a
    /// page is kept for "Reopen closed tab". Closing the agent's tab leaves the agent none: its next page opens a new one.
    /// False when there is no such tab.
    pub fn close(&mut self, id: &str) -> bool {
        let Some(at) = self.tabs.iter().position(|t| t.id == id) else { return false };
        let tab = self.tabs.remove(at);
        if self.front.as_deref() == Some(id) {
            self.front = self.tabs.get(at).or_else(|| self.tabs.last()).map(|t| t.id.clone());
        }
        if self.agent.as_deref() == Some(id) {
            self.agent = None;
        }
        if !tab.url.is_empty() {
            self.closed.retain(|c| c.url != tab.url);
            self.closed.push(tab);
            if self.closed.len() > MAX_CLOSED {
                self.closed.remove(0);
            }
        }
        true
    }

    /// Closes every tab but `id`, which comes to the front.
    pub fn close_others(&mut self, id: &str) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        let others: Vec<String> = self.tabs.iter().filter(|t| t.id != id).map(|t| t.id.clone()).collect();
        for other in others {
            self.close(&other);
        }
        self.front = Some(id.to_string());
        true
    }

    /// Opens the latest closed tab again, in front, under a new id. None when nothing was closed.
    pub fn reopen(&mut self) -> Option<String> {
        let tab = self.closed.pop()?;
        let id = self.add(tab.url, tab.title);
        self.front = Some(id.clone());
        Some(id)
    }

    /// The tab a page the person asked for from a reply's card shows in: one already there brought to the front, else the
    /// blank tab in front, else a new tab in front. Its page loads with `record`.
    pub fn show(&mut self, url: &str) -> String {
        let id = match self.tabs.iter().find(|t| t.url == url) {
            Some(t) => t.id.clone(),
            None => match self.front_tab().filter(|t| t.url.is_empty()) {
                Some(t) => t.id.clone(),
                None => self.add(String::new(), String::new()),
            },
        };
        self.front = Some(id.clone());
        id
    }

    /// The tab the agent's next page opens in: its own while it has one, else the tab in front when that is blank (or there
    /// is none), else a new tab at the end, which stays behind the one the person is on. Never the person's page.
    pub fn agent_tab(&mut self) -> String {
        if let Some(id) = self.agent.clone().filter(|id| self.get(id).is_some()) {
            return id;
        }
        let id = match self.front_tab().filter(|t| t.url.is_empty()) {
            Some(t) => t.id.clone(),
            None => {
                let id = self.add(String::new(), String::new());
                self.front.get_or_insert_with(|| id.clone());
                id
            }
        };
        self.agent = Some(id.clone());
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
    fn the_agent_opens_its_own_tab_unless_the_front_one_is_blank_and_never_takes_the_person_s() {
        let mut tabs = Tabs::default();
        let first = tabs.agent_tab();
        assert_eq!((tabs.front.as_deref(), tabs.agent.as_deref()), (Some(first.as_str()), Some(first.as_str())), "a pane with no tab");
        tabs.record(Some(&first), "https://example.org/agent", None);
        assert_eq!(tabs.agent_tab(), first, "its next page stays in its tab");
        // The person opens a tab of their own and a page in it: the agent keeps its tab, behind it.
        let mine = tabs.new_tab();
        tabs.record(Some(&mine), "https://example.org/mine", None);
        assert_eq!(tabs.agent_tab(), first);
        // Its tab closed, the agent's next page opens a new tab that stays behind the person's.
        tabs.close(&first);
        let next = tabs.agent_tab();
        assert!(next != mine && next != first);
        assert_eq!(tabs.front.as_deref(), Some(mine.as_str()), "the person's tab stays in front");
        // A blank tab in front is the agent's to use.
        tabs.agent = None;
        let blank = tabs.new_tab();
        assert_eq!(tabs.agent_tab(), blank);
    }

    #[test]
    fn closing_moves_the_front_to_the_next_tab_and_a_closed_page_comes_back_once() {
        let mut tabs = Tabs::default();
        let ids: Vec<String> = (0..3)
            .map(|n| {
                let id = tabs.new_tab();
                tabs.record(Some(&id), &format!("https://example.org/{n}"), Some(&format!("Page {n}")));
                id
            })
            .collect();
        tabs.bring(&ids[1]);
        assert!(tabs.close(&ids[1]));
        assert_eq!(tabs.front.as_deref(), Some(ids[2].as_str()), "the one after it");
        tabs.close(&ids[2]);
        assert_eq!(tabs.front.as_deref(), Some(ids[0].as_str()), "the one before it when it was the last");
        let again = tabs.reopen().unwrap();
        assert_eq!(tabs.front_tab().map(|t| (t.url.as_str(), t.title.as_str())), Some(("https://example.org/2", "Page 2")));
        assert!(again != ids[2], "under a new id");
        assert_eq!(tabs.reopen().map(|id| tabs.get(&id).unwrap().url.clone()).as_deref(), Some("https://example.org/1"));
        assert_eq!(tabs.reopen(), None);
        tabs.close_others(&ids[0]);
        assert_eq!(tabs.tabs.len(), 1);
        assert!(!tabs.close("t99") && !tabs.bring("t99"));
        let blank = tabs.new_tab();
        tabs.close(&blank);
        assert_eq!(tabs.closed.iter().filter(|t| t.url.is_empty()).count(), 0, "a blank tab is not kept");
    }

    #[test]
    fn a_card_s_page_shows_in_its_own_tab_when_one_has_it() {
        let mut tabs = Tabs::default();
        let agent = tabs.agent_tab();
        tabs.record(Some(&agent), "https://example.org/guide", None);
        let mine = tabs.new_tab();
        tabs.record(Some(&mine), "https://example.org/mine", None);
        assert_eq!(tabs.show("https://example.org/guide"), agent);
        assert_eq!(tabs.front.as_deref(), Some(agent.as_str()));
        let other = tabs.show("https://example.org/other");
        assert!(other != agent && other != mine);
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
