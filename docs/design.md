# Faber design

Decided on 2026-10-10. Live mock: the default theme and the Appearance screen.

## Default theme: Mono

- Calm, dark and quiet, with generous spacing and rounded corners, in the spirit of Claude Desktop.
- **Monochrome:** greys only, no accent color. Live information (model line, quota bars, running work) is told apart
  by weight and position, not by hue.
- **Interface in Geist Mono** (labels, navigation, model line, quota, buttons). **Answers in Geist**, a reading face.

Reference tokens (dark mode):

| Token | Value | Use |
| :--- | :--- | :--- |
| `bg` | `#15171b` | window |
| `side` | `#111316` | sidebar |
| `surface` | `#1d2025` | inputs, user bubbles, chips |
| `surface2` | `#181b1f` | cards |
| `line` | `#262a30` | borders |
| `fg` | `#e4e6ea` | main text |
| `soft` | `#b8bcc4` | secondary text |
| `muted` | `#7c828c` | hints and metadata |
| `accent` | `#d0d3d9` | the "accent" is a light grey in Mono |

## Settings > Appearance

| Setting | Options |
| :--- | :--- |
| Preset | Mono (default), Studio, Cockpit |
| Mode | Dark, Black, Light, Follow system |
| Accent color | none (monochrome, default), sage, cobalt, amber, rose, violet, teal, any color |
| Interface font | Geist Mono, JetBrains Mono, Sans |
| Reading font | Geist, Mona Sans, same as interface |
| Density | Comfortable, Compact |
| Corners | Round, Square |

Themes are plain files, so people can share them and the curated catalog can offer community themes.

## How automated runs see the app

The UI runs in Electron's Chromium, so tests and judges drive the real app with Playwright's Electron support:
`npm run test:e2e` launches the built app with `FABER_HIDDEN=1` (the window renders offscreen and never takes the
user's screen), reads the page, clicks and types like a person, and saves screenshots to `test-results/e2e/`. CI runs
it on all three systems and keeps the screenshots as artifacts. Anything outside the page (tray, notifications,
installer, updates) is checked through the operating system's accessibility layer and file versions.
