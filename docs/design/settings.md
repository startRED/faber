# Settings

How the Settings screen looks and behaves. The first run and the tutorial are in [first-run.md](first-run.md).

## Settings

- Keyboard: Back takes focus; closing restores the opener (composer, Settings or quota). Tab and rings cover enabled controls; the custom color input explicitly carries the shared focus-visible ring to its wheel on macOS too. Choices keep focus through saves and translations. Checked at 720 px.
- Settings replaces the shell, as before, with a list of five sections on the left: General, Projects, Appearance,
  Accounts and limits and Privacy, and "Faber <version>" once at the bottom of the list. Ctrl comma opens it on General,
  the quota of a vendor in the sidebar opens it on Accounts and limits, and Esc or Back closes it. With a native options
  list open, the first Esc dismisses only that list and keeps Settings and its focused select in place. Switching sections is
  a short cross-fade (the owner, 2026-10-17, reversing R169): the old section leaves as an inert copy while the new one
  arrives. The departing copy preserves each native select's current choice, including the language label;
  copying that state does not save or change the live preference. The new section opens scrolled to its top, and with reduced or no motion it is instant. All sections share one content width (`--st-w`: 720 px, growing with the window to 820 px, centred right of the list; the Appearance section keeps its own two columns), and the section title is not drawn again above the cards (the list names it; screen readers still get the heading).
- One grammar for every section (`ui/controls.tsx`). A group is a card (`--s1` with an inset `--line`, `--r-md`) under a
  small title; inside it every option is one row: the name and one short line (one line, 70 characters at most) at the left
  and the control at the right, rows divided by hairlines. The longer text of an option unfolds under its row from a small
  round "?" next to the name (`aria-expanded`, `aria-controls`, one accessible name, `set.learnMore`); there is no
  "Learn more" link. A fold (`Fold`) is a row whose title is a button; what is inside it is not in the page while it is
  closed. Switches are native checkboxes with `role="switch"`; segmented controls and selects sit in the control slot; the
  help button and checkbox hit areas use the small-control height token (28 px at standard density); the switch track
  remains 18 px high. The same checkbox in Catalog keeps its centre aligned with the quick action button using
  the two control-height tokens, without native margins. Forced colours uses the native checkbox so its checked state remains visible. The
  mode menu is a select-only combobox whose open list shows each choice with its own line. Text fields share one class,
  `.field`. Under 960 px the control slot moves under the name.
- Settings uses plain names in all five languages: automatic actions (with "hooks" explained beside the switch),
  Stored copy for saved web results, and named limit periods. The hook hint and help explain that Claude Code keeps automatic actions off outside Full access to protect saved approvals; the stored switch still applies to other backends. Incomplete readings say "at least" instead of a symbol.
  The account total keeps the shared "in API value" wording and immediately explains the estimate and that it is not a bill.
  Both access menus show every choice's description, with Ask's existing approvals and Full access's file, command and
  internet access stated in plain words. Long security warnings and all five existing modes, including Plan, remain. The Full access explanation also says in every language that the AI can bypass approval of project preferences and lessons.
  The permission row's closed details explain the requested CLI policies and their limits; they do not claim machine isolation has been verified.
- General has, in groups: *You* (language, name); *How the AI works* (Permissions as a menu of the five modes, Web search,
  automatic actions (hooks), "Full trust for Faber’s own setup" (one switch with one short line and a "?": off by default; it only skips the cards for the agent’s catalog installs, installed-tool switches, reversible removals and bounded web-search preference changes, and never covers sending, publishing, paying or deleting; it is written only here, and it is not a wall against an agent in Full access, which can edit the settings file as it can edit any file), Chat working folder with the recent folders where its control exists, Reply style for both AIs, and a closed "Codex" fold with
  reasoning summary only); *Notifications and system* (Notify me, Play a sound, and on Windows keep the
  computer awake while Faber works and start Faber at sign-in); *Updates* (one switch, "Receive previews", off by default,
  with one short line; it acts at the next update check and turning it off never installs an older version, and "Check now" reports a failed or already running check rather than claiming the app is up to date). "Don't ask" is the default for new and
  existing settings; "Ask before commands" preserves the previous behavior. Choices save at once and stay at the saved
  value if the save fails. R107 applies the same default to Chat. Chat uses its chosen canonical folder
  (fixed before the first message), or private per-conversation scratch; deletion/draft cleanup never removes a chosen
  folder. General explains the rule and offers the folder choice.
- Reply style is one select row for both AIs in General, before the Codex fold: AI's own way (default), Short, Explained,
  My own words. My own words opens a three-line text field below the row, named by the row's label, limited to 400
  characters. Leaving the field or closing Settings (including Escape while typing) saves trimmed plain text;
  switching away keeps it for next time. Failed saves restore
  the saved choice or text. The short line says it applies from the next message in Chat and Projects; the closed "?"
  explains the limit and that a running reply keeps its style. Like a permission change, a reply-style change applies
  from the next turn of an idle conversation, after the running turn otherwise. The existing settings-change path
  closes idle sessions and retires affected Codex servers; running checks, a scheduled compaction and its continuation,
  messages being sent and Keep going rests still count as work. A queue left by Stop stays saved but does not keep a
  retired session alive; sending it later starts a session with the new settings. A pending compaction and its
  continuation follow the
  conversation across session replacements, including an automatic correction after failed checks. The next launch
  resumes the history with the latest style
  through a new Claude Code process or a fresh Codex app-server. The old session closes when its work ends, also after
  Stop, a check result, Keep going ending by itself or being turned off, archive or a quota stop. The shared running-state
  refresh handles that cleanup; removing a conversation closes its session too. The old server closes with its last
  session. Replacing a session for the next message lets background commands finish on the original driver, including
  commands announced after replacement: their output and Stop still belong to that driver;
  an old thread waiting for that shared server to close does not keep the current idle session alive.
  No new motion or restart prompt.
- Projects contains only each project's own closed card: checks, separate copies and one switch, "Fix a broken goal by itself" (off by default; its short line always shows the daily limit and today's spending, US$2 a day per project, and its "?" says what runs and when). The goals themselves are on the project's page. Skills and tool servers are managed in the sidebar's [Catalog](catalog.md) tab; the old catalog cards are removed (R344 supersedes R247 placement).
  Refreshing a project's saved data retains its open card, unsent check editor and keyboard focus by project identity;
  removing an earlier project cannot transfer those controls to another project. Saved copy preferences still update.
- Permission choices and their safeguards are described in [permissions.md](permissions.md).
- Appearance is a draft, in the same cards and rows as the other sections. The first card, Look, has the six looks as small
  live renders (one look card, shared with the first run) and the accent with a hex field; the second, Text and layout, has
  the background, the two fonts, text size, density, corners and motion. A closed "Advanced" fold holds the measured contrast
  of text, faint text and the accent, the three effect switches, the fixed-meaning colours and the theme file. The controls
  change the draft, and the preview panel on the right wears the answer of `preview_look`. The panel
  says "Not applied yet" until "Use this look", which sits in a bar that stays in view. "Back to default" returns the
  draft to Dark Room. The three meaning colours are shown read-only, with their shapes and words.
- A refused accent, or a look with a failing pair, shows its sentence in the bar and "Use this look" stays as it was; a
  refused accent puts the draft back to the last one the shell accepted.
- An account counts as connected only when its CLI says someone is signed in; an installed CLI that says nobody is, or
  that cannot answer, is shown as such. The check runs each time the section opens.
- The preview draws the effect of the new conversation (a still picture of the beam, the loom or the line) and the two
  lights, so each of the three switches shows what it does before "Use this look".
- A theme file is written beside its destination and renamed over it only when the whole write worked, so a full disk
  leaves an old file as it was. A failed read or write shows words of Faber's own, never the system's.
- "Manage accounts" opens the account cards alone. Closing them (or Esc) brings Settings back on the same section, with
  the draft that was not applied. An install failure explains what happened and offers Try again and the official page, including when the request fails before reporting progress.
- Accounts and limits has one card per AI (the former Connected accounts and Usage limits, one section): its mark, name,
  the plan with its first letter capital, and one status word in a pill that is amber only when a window is really near
  its limit (C002; fast use is said calmly). Then one row per quota window the vendor has reported (name, used share, a
  bar with a mark at the share of the window's clock time gone, and the reset time in days and hours, such as "3 d 13 h"); an old reading, or none, has no bar and reads "No reading yet", and the card says once, above its spend line, to check the limits with the provider. The card ends
  with Faber's own spend today in API value, written the language's way with two decimals. Every note about how the
  figures are read (counting delay, shared account, incomplete meters, learning, foreign use, extra usage, each window's
  projection and age) is behind one "Details" fold per card. Inside it, a second fold shows the last 30 days of Faber’s API value as daily bars and explains that replies of unknown cost are excluded. "Manage accounts" appears once in the section. After the cards comes one more card titled "Auto", with one row, "What Auto learned": a Forget button (it reads "Forgotten" for 2.5 s and cannot be pressed twice in that time; an error shows under the row in the existing `.note.err` line). It sits here because Auto's picks follow these limits and Auto has no section of its own.
- Privacy starts with one Blocked sites row: one short line, details behind its ?, a field limited to 253 characters and Add, then the saved hosts with a named Remove button each (at most 50). It uses the existing field, button and option grammar, wraps long hosts, and stacks at the shared narrow breakpoint. Add and Remove wait for saving; a failed save retains the draft/list. Successful removal returns focus to the add field. The list blocks host names and their subdomains only in Faber’s browser. No new motion, process or timer. Privacy then has three statements: telemetry is off, history stays on this PC, and Faber never reads the CLI logins.

## Known incoherences

- The Electron pane enforces Blocked sites for direct opens through the Rust host only. Page links, history and redirects need the Electron move’s per-hop navigation hook. Tauri applies the same check to every navigation callback the platform reports; unreported redirect hops cannot be checked.
