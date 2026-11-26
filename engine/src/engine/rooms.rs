//! The project as a short list of rooms (task 206): its screens and its major parts, read from the project map
//! (`engine-core/src/map.rs`), each with the files its recent answers changed (the rewind history's saved changes, never a
//! git process) and the newest before and after picture of an answer that changed it (`pictures.rs`). The room the newest
//! answer changed is marked, so the screen lights it. One read per open of the panel and per answer end: one scan of the
//! folder, at most `RECENT_ANSWERS` saved-change lists and one listing of the answers' pictures.
use super::*;
use crate::map::{Item, Kind};

/// Rooms one list holds at most: a short list a person reads at a glance.
const MOST_ROOMS: usize = 12;
/// Screens among them at most; the rest are parts.
const MOST_SCREENS: usize = 8;
/// The newest answers whose changes are read.
const RECENT_ANSWERS: usize = 20;
/// Changed files one room lists, newest first.
const RECENT_FILES: usize = 5;
/// Folders that hold a project's screens, one per file.
const SCREEN_FOLDERS: [&str; 5] = ["pages", "routes", "screens", "views", "app"];
/// Folders that only hold sources: a part is the folder below them.
const SOURCE_FOLDERS: [&str; 4] = ["src", "source", "lib", "app"];

/// One room of the project, as the map panel shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    /// The screen's or part's name, from its file or folder. Empty for the home screen, which the screen names itself.
    pub name: String,
    /// `screen` or `part`.
    pub kind: &'static str,
    /// The file (a screen) or folder (a part) it covers, relative to the project's folder, with `/`.
    pub at: String,
    /// The files in it recent answers changed, newest first, at most `RECENT_FILES`.
    pub recent: Vec<String>,
    /// The newest answer that changed it: the person's message, the answer's own id and its place in the conversation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_by: Option<ChangedBy>,
    /// The newest before and after picture of an answer that changed it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<RoomPicture>,
    /// The newest answer of the conversation changed it.
    pub touched: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedBy {
    pub message: String,
    pub answer: String,
    pub index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomPicture {
    pub answer: String,
    pub page: AnswerPage,
}

/// What one answer changed: the person's message, the answer (none while it has not started), its index, and the files.
#[derive(Debug, Clone, Default)]
pub(super) struct Changed {
    pub message: String,
    pub answer: Option<(String, usize)>,
    pub files: Vec<String>,
    /// The newest message of the conversation the person sent.
    pub newest: bool,
}

impl Engine {
    /// The rooms of a project conversation's folder, read outside the engine's lock. A chat, or a folder that cannot be
    /// read, has none.
    pub fn project_rooms(&self, id: &str) -> Vec<Room> {
        let Some((folder, asked)) = self.with(|c| {
            let conversation = c.load(id)?;
            if conversation.area != Area::Project {
                return None;
            }
            let messages = &conversation.messages;
            let newest = messages.iter().rposition(|m| m.role == Role::User);
            let mut asked = Vec::new();
            for (at, m) in messages.iter().enumerate().rev() {
                if asked.len() == RECENT_ANSWERS {
                    break;
                }
                if m.role != Role::User || !m.rewind.as_ref().is_some_and(|point| !point.coverage.unavailable) {
                    continue;
                }
                let answer = messages[at + 1..]
                    .iter()
                    .position(|n| n.role == Role::Assistant)
                    .map(|n| (messages[at + 1 + n].id.clone(), at + 1 + n));
                asked.push(Changed { message: m.id.clone(), answer, files: Vec::new(), newest: Some(at) == newest });
            }
            Some((c.conversation_folder(id)?, asked))
        }) else {
            return Vec::new();
        };
        let changed: Vec<Changed> = asked
            .into_iter()
            .map(|mut one| {
                one.files =
                    self.answer_changes(id, &one.message, 0).map(|c| c.files.into_iter().map(|f| f.path).collect()).unwrap_or_default();
                one
            })
            .collect();
        rooms(&crate::map::scan(&folder), &changed, &self.answer_pictures(id))
    }
}

/// The rooms `items` (one scan of the project) make, with what `changed` (newest answer first) did to them and the pictures
/// of those answers. Screens come first, then parts, each by place; at most `MOST_ROOMS`.
pub(super) fn rooms(items: &[Item], changed: &[Changed], pictures: &BTreeMap<String, Vec<AnswerPage>>) -> Vec<Room> {
    let screens: Vec<&str> = items.iter().filter(|i| i.kind == Kind::Screen).map(|i| i.file.as_str()).collect();
    let routed: Vec<&str> = screens.iter().copied().filter(|file| routed(file)).collect();
    let mut chosen = if routed.is_empty() && screens.len() <= MOST_SCREENS { screens } else { routed };
    // The home screen first, then by place in any case.
    chosen.sort_by_key(|file| (!screen_name(file).is_empty(), file.to_lowercase()));
    chosen.dedup();
    chosen.truncate(MOST_SCREENS);
    let mut out: Vec<Room> = chosen.iter().map(|file| room(screen_name(file), "screen", file)).collect();
    // A part is weighed by what the map found in it; the heaviest fill the list, then go by place.
    let mut weight: BTreeMap<String, usize> = BTreeMap::new();
    for item in items.iter().filter(|i| !chosen.contains(&i.file.as_str())) {
        if let Some(part) = part_of(&item.file) {
            *weight.entry(part).or_default() += 1;
        }
    }
    let mut parts: Vec<(String, usize)> = weight.into_iter().collect();
    parts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    parts.truncate(MOST_ROOMS - out.len());
    parts.sort();
    out.extend(parts.into_iter().map(|(at, _)| room(at.rsplit('/').next().unwrap_or(&at).to_string(), "part", &at)));
    for (n, answer) in changed.iter().enumerate() {
        for file in &answer.files {
            let Some(room) = home_of(&mut out, file) else { continue };
            room.touched |= n == 0 && answer.newest;
            if room.recent.len() < RECENT_FILES && !room.recent.contains(file) {
                room.recent.push(file.clone());
            }
            let Some((id, index)) = &answer.answer else { continue };
            if room.changed_by.is_none() {
                room.changed_by = Some(ChangedBy { message: answer.message.clone(), answer: id.clone(), index: *index });
            }
            if room.picture.is_none()
                && let Some(page) = pictures.get(id).and_then(|pages| pages.iter().find(|p| p.before.is_some() && p.after.is_some()))
            {
                room.picture = Some(RoomPicture { answer: id.clone(), page: page.clone() });
            }
        }
    }
    out
}

fn room(name: String, kind: &'static str, at: &str) -> Room {
    Room { name, kind, at: at.to_string(), recent: Vec::new(), changed_by: None, picture: None, touched: false }
}

/// A screen file inside a folder of screens, or a Next-style `page` file.
fn routed(file: &str) -> bool {
    let mut parts: Vec<&str> = file.split('/').collect();
    let name = parts.pop().unwrap_or_default();
    name.split('.').next().map(|s| s.trim_start_matches('+')) == Some("page")
        || parts.iter().any(|dir| SCREEN_FOLDERS.contains(dir) && *dir != "app")
}

/// A screen's name: its file's stem, or its folder's for an `index` or `page` file. The top screen has none (home).
fn screen_name(file: &str) -> String {
    let mut parts: Vec<&str> = file.split('/').collect();
    let stem = parts.pop().unwrap_or_default().split('.').next().unwrap_or_default().trim_start_matches('+');
    if !["index", "page"].contains(&stem) {
        return stem.to_string();
    }
    match parts.pop() {
        Some(dir) if !SCREEN_FOLDERS.contains(&dir) && !SOURCE_FOLDERS.contains(&dir) => dir.to_string(),
        _ => String::new(),
    }
}

/// The part a file belongs to: its first folder, or the one below a folder of sources. A file at the top has none.
fn part_of(file: &str) -> Option<String> {
    let folders: Vec<&str> = file.split('/').collect();
    let folders = &folders[..folders.len() - 1];
    let first = *folders.first()?;
    if SOURCE_FOLDERS.contains(&first)
        && let Some(next) = folders.get(1)
    {
        return Some(format!("{first}/{next}"));
    }
    Some(first.to_string())
}

/// The room a changed file is in: the screen it is, else the part whose folder holds it most closely.
fn home_of<'a>(rooms: &'a mut [Room], file: &str) -> Option<&'a mut Room> {
    if let Some(at) = rooms.iter().position(|r| r.kind == "screen" && r.at == file) {
        return rooms.get_mut(at);
    }
    rooms
        .iter_mut()
        .filter(|r| r.kind == "part" && file.strip_prefix(r.at.as_str()).is_some_and(|rest| rest.starts_with('/')))
        .max_by_key(|r| r.at.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(files: &[&str]) -> (crate::workspace::TestRoot, Vec<Item>) {
        let root = crate::workspace::TestRoot::new();
        for file in files {
            let path = root.path().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let text = if file.ends_with("package.json") {
                r#"{ "scripts": { "dev": "vite" } }"#
            } else if file.ends_with(".rs") {
                "pub fn run() {}\n"
            } else {
                "export function Thing() {}\n"
            };
            std::fs::write(path, text).unwrap();
        }
        let items = crate::map::scan(root.path());
        (root, items)
    }

    fn summary(rooms: &[Room]) -> Vec<(String, &'static str, String)> {
        rooms.iter().map(|r| (r.name.clone(), r.kind, r.at.clone())).collect()
    }

    #[test]
    fn rooms_are_the_screens_and_major_parts_of_a_fixture_project() {
        let (_root, items) = fixture(&[
            "package.json",
            "src/pages/index.tsx",
            "src/pages/Login.tsx",
            "src/pages/settings/index.tsx",
            "src/components/Button.tsx",
            "src/components/Card.tsx",
            "src/lib/api.ts",
            "server/main.rs",
            "server/routes.rs",
            "node_modules/dep/index.js",
            "tests/app.test.ts",
        ]);
        assert_eq!(
            summary(&rooms(&items, &[], &BTreeMap::new())),
            [
                ("".into(), "screen", "src/pages/index.tsx".into()),
                ("Login".into(), "screen", "src/pages/Login.tsx".into()),
                ("settings".into(), "screen", "src/pages/settings/index.tsx".into()),
                ("server".into(), "part", "server".into()),
                ("components".into(), "part", "src/components".into()),
                ("lib".into(), "part", "src/lib".into()),
            ]
        );
        // With no folder of screens, a few screen files are each a room; many are left to their parts.
        let (_root, items) = fixture(&["ui/App.tsx", "ui/Settings.tsx", "ui/api.ts"]);
        assert_eq!(
            summary(&rooms(&items, &[], &BTreeMap::new())),
            [
                ("App".into(), "screen", "ui/App.tsx".into()),
                ("Settings".into(), "screen", "ui/Settings.tsx".into()),
                ("ui".into(), "part", "ui".into())
            ]
        );
        let many: Vec<String> = (0..20).map(|n| format!("ui/Screen{n}.tsx")).collect();
        let (_root, items) = fixture(&many.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(summary(&rooms(&items, &[], &BTreeMap::new())), [("ui".into(), "part", "ui".into())]);
    }

    #[test]
    fn rooms_stay_a_short_list_however_big_the_project() {
        let files: Vec<String> = (0..30)
            .flat_map(|n| [format!("part{n:02}/a.rs"), format!("src/pages/Page{n:02}.tsx")])
            .chain((0..5).map(|n| format!("part00/x{n}.rs")))
            .collect();
        let (_root, items) = fixture(&files.iter().map(String::as_str).collect::<Vec<_>>());
        let list = rooms(&items, &[], &BTreeMap::new());
        assert_eq!(list.len(), MOST_ROOMS);
        assert_eq!(list.iter().filter(|r| r.kind == "screen").count(), MOST_SCREENS);
        // The heaviest part always makes the list.
        assert!(list.iter().any(|r| r.at == "part00"));
    }

    #[test]
    fn rooms_carry_recent_changes_the_answer_to_go_back_to_its_picture_and_the_touched_room() {
        let (_root, items) =
            fixture(&["src/pages/Login.tsx", "src/pages/Home.tsx", "src/components/Button.tsx", "src/components/Card.tsx", "README.md"]);
        let changed = |message: &str, answer: Option<(&str, usize)>, files: &[&str], newest| Changed {
            message: message.into(),
            answer: answer.map(|(id, at)| (id.into(), at)),
            files: files.iter().map(|f| f.to_string()).collect(),
            newest,
        };
        let page = AnswerPage { page: 1, before: Some("before-a2-1.jpg".into()), after: Some("after-a2-1.jpg".into()) };
        let pictures = BTreeMap::from([
            ("a2".to_string(), vec![page.clone()]),
            ("a3".to_string(), vec![AnswerPage { page: 1, before: None, after: Some("after-a3-1.jpg".into()) }]),
        ]);
        let list = rooms(
            &items,
            &[
                changed("q3", Some(("a3", 5)), &["src/pages/Login.tsx", "src/components/Card.tsx", "README.md"], true),
                changed("q2", Some(("a2", 3)), &["src/pages/Login.tsx", "src/components/Button.tsx"], false),
                changed("q1", Some(("a1", 1)), &["src/pages/Home.tsx"], false),
            ],
            &pictures,
        );
        let by = |at: &str| list.iter().find(|r| r.at == at).unwrap();
        let login = by("src/pages/Login.tsx");
        assert!(login.touched);
        assert_eq!(login.recent, ["src/pages/Login.tsx"]);
        assert_eq!(login.changed_by, Some(ChangedBy { message: "q3".into(), answer: "a3".into(), index: 5 }));
        // The newest answer kept only an after picture: the room shows the newest pair.
        assert_eq!(login.picture, Some(RoomPicture { answer: "a2".into(), page }));
        let parts = by("src/components");
        assert!(parts.touched);
        assert_eq!(parts.recent, ["src/components/Card.tsx", "src/components/Button.tsx"]);
        let home = by("src/pages/Home.tsx");
        assert!(!home.touched && home.picture.is_none());
        assert_eq!(home.changed_by.as_ref().map(|c| c.index), Some(1));
        // An answer that is not the newest message lights nothing, and a prompt with no answer yet offers no way back.
        let list = rooms(&items, &[changed("q4", None, &["src/pages/Home.tsx"], false)], &pictures);
        let home = list.iter().find(|r| r.at == "src/pages/Home.tsx").unwrap();
        assert!(!home.touched && home.changed_by.is_none());
        assert_eq!(home.recent, ["src/pages/Home.tsx"]);
    }

    #[test]
    fn rooms_names_and_parts_follow_the_folders() {
        assert_eq!(screen_name("app/page.tsx"), "");
        assert_eq!(screen_name("app/login/page.tsx"), "login");
        assert_eq!(screen_name("src/routes/+page.svelte"), "");
        assert_eq!(screen_name("src/views/Cart.vue"), "Cart");
        assert!(routed("app/login/page.tsx") && routed("src/views/Cart.vue") && !routed("app/Button.tsx"));
        assert_eq!(part_of("src/main.rs"), Some("src".into()));
        assert_eq!(part_of("engine/src/x.rs"), Some("engine".into()));
        assert_eq!(part_of("package.json"), None);
    }
}
