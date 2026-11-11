//! The screens a page is checked on (R245): a short set of common sizes per kind of screen, and which kinds matter for a
//! project, read from what its folder is (a web app, a desktop app, a phone app), so Faber's browser checks a thing at the
//! sizes of where it will run and nobody has to set it.

use std::path::Path;

/// A kind of screen a thing runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Pc,
    Mac,
    Tablet,
    Phone,
}

impl Screen {
    pub const ALL: [Screen; 4] = [Screen::Pc, Screen::Mac, Screen::Tablet, Screen::Phone];

    /// The name the agent gives it in the tool's input.
    pub fn name(self) -> &'static str {
        match self {
            Screen::Pc => "pc",
            Screen::Mac => "mac",
            Screen::Tablet => "tablet",
            Screen::Phone => "phone",
        }
    }

    /// The words the agent reads and the screen matches (`ui/Site.tsx`).
    pub fn words(self) -> &'static str {
        match self {
            Screen::Pc => "a PC",
            Screen::Mac => "a MacBook",
            Screen::Tablet => "a tablet",
            Screen::Phone => "a phone",
        }
    }

    pub fn from_name(name: &str) -> Option<Screen> {
        Screen::ALL.into_iter().find(|s| s.name() == name.trim().to_ascii_lowercase())
    }
}

/// One screen size: the page's width and height in CSS pixels and how many device pixels each has.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub screen: Screen,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

impl Viewport {
    /// Touch and a phone's layout rules: tablets and phones.
    pub fn mobile(&self) -> bool {
        matches!(self.screen, Screen::Tablet | Screen::Phone)
    }

    /// `390x844`, the name the agent gives it.
    pub fn name(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    /// `a phone, 390x844 at 3x`: one line the agent reads and the screen matches.
    pub fn words(&self) -> String {
        let scale = if self.scale.fract() == 0.0 {
            format!("{}", self.scale)
        } else {
            format!("{:.2}", self.scale).trim_end_matches('0').to_string()
        };
        format!("{}, {} at {scale}x", self.screen.words(), self.name())
    }
}

const fn size(screen: Screen, width: u32, height: u32, scale: f64) -> Viewport {
    Viewport { screen, width, height, scale }
}

/// The common sizes, a few per kind of screen, the most used first: Full HD and its 125% scaling, the usual laptops, the
/// MacBook Air and Pro, an iPad Air, an iPhone and a common Android phone.
pub const SIZES: [Viewport; 9] = [
    size(Screen::Pc, 1920, 1080, 1.0),
    size(Screen::Pc, 1536, 864, 1.25),
    size(Screen::Pc, 1366, 768, 1.0),
    size(Screen::Pc, 1280, 720, 1.0),
    size(Screen::Mac, 1440, 900, 2.0),
    size(Screen::Mac, 1512, 982, 2.0),
    size(Screen::Tablet, 820, 1180, 2.0),
    size(Screen::Phone, 390, 844, 3.0),
    size(Screen::Phone, 360, 800, 2.0),
];

/// The sizes of these kinds of screen, in the order of [`SIZES`].
pub fn sizes_of(screens: &[Screen]) -> Vec<Viewport> {
    SIZES.into_iter().filter(|v| screens.contains(&v.screen)).collect()
}

/// A size the agent named: one of [`SIZES`] by its `390x844` name, or a kind of screen for its first size.
pub fn named(name: &str) -> Option<Viewport> {
    let name = name.trim().to_ascii_lowercase().replace(['×', ' '], "x");
    SIZES.into_iter().find(|v| v.name() == name).or_else(|| Screen::from_name(&name).and_then(|s| sizes_of(&[s]).first().copied()))
}

/// What a project is, as far as where it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Web,
    Desktop,
    Mobile,
    /// One codebase for desktop and phones (Tauri or Capacitor with both, Flutter).
    Everywhere,
}

impl Kind {
    pub fn words(self) -> &'static str {
        match self {
            Kind::Web => "a web app",
            Kind::Desktop => "a desktop app",
            Kind::Mobile => "a phone app",
            Kind::Everywhere => "an app for desktops and phones",
        }
    }

    /// The screens it runs on. A web page can open anywhere; a desktop app is a window on a PC or a Mac; a phone app also
    /// runs on tablets.
    pub fn screens(self) -> &'static [Screen] {
        match self {
            Kind::Web | Kind::Everywhere => &Screen::ALL,
            Kind::Desktop => &[Screen::Pc, Screen::Mac],
            Kind::Mobile => &[Screen::Tablet, Screen::Phone],
        }
    }
}

/// The most of a manifest Faber reads to tell a project's kind.
const MAX_MANIFEST: u64 = 512 * 1024;

/// What the project in `folder` is, from the files at its top (and one folder down for a package manifest, the usual
/// `app/`, `web/`, `frontend/` or `client/`): a desktop shell (Tauri, Electron, Neutralino, NW.js), a phone app (React
/// Native, Expo, Capacitor, Ionic, NativeScript, an `android` and an `ios` folder), Flutter for both, and a web app when
/// none says otherwise. Only names and small manifests are read; nothing runs.
pub fn kind_of(folder: &Path) -> Kind {
    let has = |path: &str| folder.join(path).exists();
    let mut deps = String::new();
    for dir in ["", "app", "web", "frontend", "client", "ui"] {
        let path = folder.join(dir).join("package.json");
        if std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_MANIFEST)
            && let Ok(text) = std::fs::read_to_string(&path)
            && let Ok(json) = serde_json::from_str::<serde_json::Value>(&text)
        {
            for key in ["dependencies", "devDependencies"] {
                if let Some(map) = json[key].as_object() {
                    for name in map.keys() {
                        deps.push_str(name);
                        deps.push('\n');
                    }
                }
            }
        }
    }
    let dep = |name: &str| deps.lines().any(|d| d == name || (name.ends_with('/') && d.starts_with(name)));
    if has("pubspec.yaml") {
        return Kind::Everywhere;
    }
    let desktop = ["electron", "@tauri-apps/api", "@tauri-apps/cli", "@neutralinojs/lib", "nw"].iter().any(|d| dep(d))
        || ["src-tauri/tauri.conf.json", "tauri.conf.json", "electron-builder.yml", "electron-builder.json", "neutralino.config.json"]
            .iter()
            .any(|f| has(f));
    let mobile = ["react-native", "expo", "@capacitor/core", "@ionic/", "@nativescript/core"].iter().any(|d| dep(d))
        || ["capacitor.config.ts", "capacitor.config.json", "ionic.config.json"].iter().any(|f| has(f))
        || (has("android") && has("ios"))
        || (has("src-tauri/gen/android") || has("src-tauri/gen/apple"));
    match (desktop, mobile) {
        (true, true) => Kind::Everywhere,
        (true, false) => Kind::Desktop,
        (false, true) => Kind::Mobile,
        (false, false) => Kind::Web,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_of_screen_has_its_common_sizes_and_the_agent_names_them_by_size_or_kind() {
        let phones: Vec<String> = sizes_of(&[Screen::Phone]).iter().map(Viewport::name).collect();
        assert_eq!(phones, ["390x844", "360x800"]);
        assert_eq!(sizes_of(&Screen::ALL).len(), SIZES.len());
        assert_eq!(named(" 1536×864 ").map(|v| (v.scale, v.mobile())), Some((1.25, false)));
        assert_eq!(named("phone").map(|v| v.name()).as_deref(), Some("390x844"), "a kind is its most common size");
        assert_eq!(named("MAC").map(|v| v.name()).as_deref(), Some("1440x900"));
        assert_eq!(named("800x600"), None, "only the common sizes");
        assert_eq!(SIZES[7].words(), "a phone, 390x844 at 3x");
        assert_eq!(SIZES[1].words(), "a PC, 1536x864 at 1.25x");
        assert!(SIZES[6].mobile() && !SIZES[4].mobile());
    }

    #[test]
    fn faber_tells_where_a_project_runs_from_its_files() {
        let kind = |files: &[(&str, &str)]| {
            let dir = tempfile::tempdir().unwrap();
            for (path, text) in files {
                let path = dir.path().join(path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            kind_of(dir.path())
        };
        assert_eq!(kind(&[]), Kind::Web, "an empty folder is a web page");
        assert_eq!(kind(&[("package.json", r#"{"dependencies":{"react":"19","vite":"7"}}"#)]), Kind::Web);
        assert_eq!(kind(&[("package.json", r#"{"devDependencies":{"electron":"38"}}"#)]), Kind::Desktop);
        assert_eq!(kind(&[("src-tauri/tauri.conf.json", "{}")]), Kind::Desktop);
        assert_eq!(kind(&[("app/package.json", r#"{"dependencies":{"expo":"54"}}"#)]), Kind::Mobile, "one folder down");
        assert_eq!(kind(&[("package.json", r#"{"dependencies":{"@ionic/react":"8"}}"#)]), Kind::Mobile);
        assert_eq!(kind(&[("android/x", ""), ("ios/x", "")]), Kind::Mobile);
        assert_eq!(kind(&[("android/x", "")]), Kind::Web, "an android folder alone says little");
        assert_eq!(kind(&[("src-tauri/tauri.conf.json", "{}"), ("src-tauri/gen/android/x", "")]), Kind::Everywhere);
        assert_eq!(kind(&[("pubspec.yaml", "name: x")]), Kind::Everywhere);
        assert_eq!(kind(&[("package.json", "not json"), ("electron-builder.yml", "")]), Kind::Desktop);
        assert_eq!(kind(&[("package.json", r#"{"dependencies":{"electron-is-dev":"3"}}"#)]), Kind::Web, "a whole name, not a prefix");
        assert_eq!(Kind::Desktop.screens(), [Screen::Pc, Screen::Mac]);
        assert_eq!(Kind::Mobile.screens(), [Screen::Tablet, Screen::Phone]);
    }
}
