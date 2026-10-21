# Visual answers

The shared reply card and running-column pane, in Chat and Projects. Native inline pieces keep their existing
controls; these rules extend the large-page and drawing viewer, never add another viewer.

## Reply pages and isolation

- A visual answer opens from its reply card in the same pane slot as a browser page: the existing back arrow, the
  answer's name and close in the head. It excludes the browser, agent, Changes, Commands, quick-question and file views.
  Its left edge uses the column's existing resize and saved width, including its keyboard controls and 720 px overlay rules.
  Version arrows in the head surround a quiet "2 of 3"; opening an answer normally starts at its latest version, while
  an older reply's card opens that exact version. At either end the unavailable arrow is disabled. Switching versions
  is instant, also with reduced or no motion. The frame fills the pane below one calm line: "This page runs offline."
  or "This page can use the web." It follows Settings' existing web choice: Live permits HTTPS resources and fetches;
  Off and Cached stay offline (Cached is the AI's search cache, not a live connection). The choice is read on opening
  and on each version switch; saving Settings leaves an open page and its input alone. There is no second switch or prompt.
  The isolated frame permits scripts but no access to Faber, its storage or commands. Only one
  frame exists while this pane is open. Versions navigate that same frame to a single-use URL on Faber's visual-page
  origin, with a fresh page state. Only the document the screen just prepared is served, at most 8 MiB and once; its
  bytes leave the native pending slot on delivery. Fetch Metadata can be absent in native callbacks; supplied
  non-frame destinations are refused, and exact single-use URLs, no CORS grant and the sandbox protect the bytes. Closing invalidates the pane's lease (late preparations are refused),
  drops its reference, sets src to about:blank and removes the frame, including its scripts. Every version keeps the
  opaque sandbox and its own response-header CSP, including the fixed visual loader's hash. The screen's resource CSP
  and original loader stay exactly as before; only frame-src admits the visual origin, never remote sites. Ordinary
  reply images, CSS, fonts and media stay blocked in both web settings. Live visual pages allow public HTTPS resources.
  Their CSP blocks HTTP and Faber's asset, IPC and visual origins; the native filters block private HTTPS destinations.
  Wry may overwrite a later WebView2 denial for its own custom protocols, so asset/IPC isolation relies on CSP and the
  opaque sandbox, never that handler's 403. Child navigation cannot leave the visual origin or exact about:blank.
  WebView2 shares Wry's filters with the Windows callback; it leaves every non-HTTPS response untouched,
  including visual, asset and IPC protocols. macOS blocks visual URLs at the top frame, allowing only the pane's child.
  Linux uses GTK's response policy to reject visual top documents while retaining offline child viewing; its
  navigation readiness never grants network access. On macOS and Linux preparation waits up to five seconds for the guard, without polling; a failed installation shows the
  existing page-opening failure instead of issuing a URL whose navigation would leave a blank frame.
  WebSocket, WebSocketStream and WebTransport are unavailable on supported desktops because they bypass HTTPS resource interception.
  These are not a DNS
  firewall: a public name resolving privately is a platform limit ([map](../product-map.md#screens-and-surfaces)).
  The shell also permits the exact empty disposal address.
  Escape inside the page, including a focused input, closes the pane. Right-click never opens the browser's menu.
  Visual documents explicitly deny child frames and workers in CSP. WebView2 follows FrameCreated at every depth:
  only the first child may navigate to a visual URL or about:blank, after its descendant guard installs; nested
  navigation is always cancelled. This needs Frame7 support and fails closed without it. macOS blocks every other
  child document in its compiled rule list. GTK rejects srcdoc, data/blob and external navigations, retaining shell resources
  and its visual-top response guard. A GTK response policy may cover subresources, so it cannot safely refuse every
  non-main response. These APIs expose
  no ancestry for initial empty realms, so their exception is only the single-use visual URL, not arbitrary documents.
  Native navigation APIs cannot prevent initial empty contexts, which can share the visual frame's opaque origin.
  The visual frame itself locks iframe/frame contentWindow and contentDocument to null and window.open to null,
  including existing named targets. ElementInternals.shadowRoot cleans a closed root before returning it. These
  protections do not depend on child initialization. A fixed parser fixture bypasses cleaning and upgrades its
  witnessed closed root's host. It avoids mXSS serialization: modern outerHTML escapes attribute angles.
  WindowProxy indexed slots cannot be overwritten safely. The loader waits for the
  native parser drain before authored replay; its hash policy blocks raw parser-time sibling scripts. A real-frame
  check probes first-script window[0], frames[0] and named access. These checks still need a pass on each desktop.

## Files written by the agent

- A successful main-agent Write, Edit, MultiEdit or fileChange call supplies the only candidate paths. Read calls,
  shell commands, failed/refused tools and child-agent tools supply none. No directory scan or polling runs.
- Only literal .html/.htm, .svg, .mmd and .mermaid names within the actual working folder count, including a
  conversation’s separate copy. Percent signs and spaces remain literal; Unix candidate filenames containing backslashes are refused.
  Unix root names keep their literal backslashes when checking an absolute path.
  Absolute candidates need lossless Unicode folder and path names; relative edits still work in non-Unicode folders.
  The folder's letter case must match exactly, except for the Windows drive letter. On Windows, any folder component ending in a dot
  or space refuses absolute candidates: removing its verbatim prefix can address a different folder. Relative names still work.
  The root handle is pinned before sending the turn and released at its end; substitution of the working folder
  cannot redirect a read. The pinned reader refuses links, reparse points, devices, protected paths and outside paths.
  On Windows that handle holds the folder and its parents against rename or deletion until the turn ends. The pin
  accepts the checked plain launch spelling and its canonical Windows prefix form without resolving a replacement.
  Files must be nonempty UTF-8 without NUL.
- Each successful edit captures at most 256 KiB per file, twelve snapshots and 1 MiB of source per reply. Identical
  consecutive bytes for a path do not add a version. The whole conversation keeps at most 8 MiB of captured source;
  later captures stop at the limit without removing earlier versions. Ordinary file links remain available.
- Captured bytes live in Message.visualFiles on disk with that reply. Message events and every conversation response
  omit them, sending only visualFileCount. Shared immutable strings avoid copying bytes before projection. Discovery
  through visual_answers returns only file locators; opening a pane version or See code requests that exact source,
  which stays out of the conversation screen store. Closing its code view or pane releases that temporary source.
  The card opens saved bytes, never the current file.
  Further edits and later replies append versions grouped by literal relative path, separately from reply titles.
  Branch and rewind/Redo retain snapshots with their messages. Reopening history never reads the project.
- Each saved version has the same reply card and See code. The card says that only this file is shown; relative
  scripts, styles and images are not served. A single self-contained page is the supported form. Live public HTTPS
  dependencies still work through the existing setting and guards; no file URL, directory or second server is exposed.

## Mermaid

- A closed mermaid reply fence or a captured .mmd/.mermaid file uses Drawing, Open, See code and the same pane.
  An unfinished fence stays code. Untitled fences are separate drawings; file diagrams group versions by their path.
- Only an opened diagram requests the official single-file Mermaid Tiny 12.2.0 build from jsDelivr, pinned with
  SHA-384 integrity and anonymous CORS. No library, tool, package or downloaded bytes ship with Faber. There are no
  dynamic imports or unchecked secondary modules in this build. Its documented exclusions are mind maps,
  architecture diagrams and KaTeX mathematics; those remain source code, as do invalid diagrams and failed downloads.
- The source is escaped into a pre element. The renderer uses strict security, HTML labels off, 16,384 characters
  and 200 edges, with these limits protected from diagram configuration. It never binds diagram click callbacks.
  Source stays visible until rendering succeeds. A localized fallback line explains Live and unsupported forms only
  while code remains visible inside the pane; successful rendering hides it. Cards carry no permanent limits line.
- Native response policy chooses Live exactly as for a reply page. Off and Cached block the library; code remains
  visible inside the pane and in See code. Changing Settings does not reload an open drawing. Linux currently stays
  offline under the existing native policy. No new permissions, loaders or frame messages exist.

## Cost and verification

- No process, timer, file watcher, idle work or extra frame is added. Snapshot I/O happens only on a successful edit,
  through bounded reads; each reply retains at most 1 MiB and a conversation at most 8 MiB of captured source.
  Streaming sends only a count, never snapshot bytes; discovery supplies no file source until a version is opened.
- Download measured on 2026-10-21: 2,811,424 raw bytes, 752,820 bytes with gzip. One isolated Node V8 compilation
  took 67.5 ms and added 7,450,048 heap bytes. This is a compilation probe, not a browser runtime/R125 measurement.
  This probe is separate from the native pane measurement below.
- Lead Windows WebView2 screen run on 2026-10-21, with FABER_LIVE_CDN=1: 27 passed, zero failed, including the real
  integrity-checked Mermaid renderer producing SVG. App and WebView2 together measured 122.1 MB before opening,
  167.2 MB after rendering and 126.4 MB at the 10.4 s post-close sample, below the 400 MB cap and 15 MiB retained margin.
  The pane's extra WebView2 process exited within about 10 s; no frame references remained after collection.
  Opening/rendering used 625 ms CPU over 638 ms including download. Idle processor use of one core measured 0.0%
  before, 3.9% with the drawing open and 0.0% after closing. These are that run's measurements, not a cross-platform guarantee.
- engine/tests/visual_files.rs replays both CLI fixtures: written versus read, rejected paths and oversize, Mermaid,
  saved versions after deletion, literal names, file/title identity, links, root substitution, Unix backslash and non-Unicode names
  and conversation caps. Real engine dispatch covers native absolute Claude Write paths and relative Codex paths in
  Chat and Projects. Windows regressions refuse normalized verbatim folder aliases, non-Unicode roots and case-different
  folder prefixes, while allowing a different drive-letter case.
  The engine’s streamed-reply regression checks 300 message events, open and move responses
  contain no captured bytes while saved versions remain intact.
- tests/ui/visual-answers.check.ts extends the real App at 720 px and reduced motion to file and Mermaid cards,
  earlier versions and pane exclusion; a file page runs the same real-frame isolation probe. Mermaid checks cover
  source escaping, pinned integrity, the existing loader and offline code fallback. Native Windows rendering and
  memory/processor checks passed in the lead's run above; this sandbox cannot run screen suites. The existing
  visual_answers_pane_memory scenario now also opens the real pinned Mermaid CDN library with Live, confirms an SVG
  and hidden code/fallback through a scoped DevTools session, measures opening CPU and idle CPU before/open/closed,
  and applies the same memory cap and 15 MiB post-close margin after collection. Only FABER_LIVE_CDN=1 enables this
  real-CDN step; otherwise it prints that Mermaid Live was not measured. Offline HTML always runs. The opted-in
  check requires internet and fails if drawing falls back to code. Task 310 is complete and parity M1 is implemented.
