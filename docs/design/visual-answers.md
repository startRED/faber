# Visual answers

The shared reply card and running-column pane, in Chat and Projects. Native inline pieces keep their existing
controls; these rules extend the large-page and drawing viewer, never add another viewer.

## Preview in the conversation

- Preview opens the saved page or drawing inside its reply card, leaving the request, answer and composer usable.
  It never opens the running column or moves focus there. Cards stay lazy until Preview is chosen; there is one
  selected visual frame across the app, so another card replaces the previous preview rather than adding frames.
- The preview shows the beginning of the page at the conversation's actual reading width in a 320 px region. It is
  a thumbnail: its frame is inert, excluded from Tab and pointer input. Expand preview opens the existing interactive
  pane at the card's exact saved version. The thumbnail is disposed before that view takes over; no form input can
  have been entered in the thumbnail. Version navigation and interaction remain in the existing expanded pane.
- Close preview removes its frame and releases the visual lease. Switching conversations also closes it. The same
  native single-use origin, sandbox, resource policy, loading timeout, error path and source limits serve both sizes.
  There is no second renderer, dependency, process, polling loop, animation or saved setting.

## Reply pages and isolation

- An expanded visual answer opens from its reply card in the same pane slot as a browser page: the existing back arrow, the
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
  origin, replacing its session-history entry with a fresh page state, one at a time: a newer version waits for the
  frame's load, and one that does not load within 15 s is prepared once more in a new frame. A failed navigation, or a second silence, removes the frame and shows the existing page-opening failure; choosing another version can recover. Only the document the screen just prepared is served, at most 8 MiB and once; its
  bytes leave the native pending slot on delivery. Fetch Metadata can be absent in native callbacks; supplied
  non-frame destinations are refused, and exact single-use URLs, no CORS grant and the sandbox protect the bytes. Closing invalidates the pane's lease (late preparations are refused),
  drops its reference, sets src to about:blank and removes the frame, including its scripts. Every version keeps the
  opaque sandbox and its own response-header CSP, including the fixed visual loader's hash. The screen's resource CSP
  admits the visual origin only in frame-src, and the media origin only in img-src and
  media-src. The screen blocks external reply resources. With Live web access, the host may fetch reply pictures,
  audio and video through the scoped media origin. Live visual pages allow public HTTPS resources.
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

## Local media in a conversation

- Markdown hard breaks can separate standalone file links. Picture frames retain an outline in forced colors.
- The screen calls `media_enter({ id })` when it starts displaying a conversation. It keeps the returned lease and
  calls `media_leave({ lease })` when leaving, including when it returns to a home or settings screen. Entering a new
  view invalidates the previous lease immediately. An old cleanup cannot close a newer lease. The shell also
  revokes capabilities when its screen closes. A restart starts with an empty capability store.
- `media_grant({ id, path })` returns an opaque, revocable URL for a relative filename. The host resolves the actual
  Chat working folder or Project copy and records its identity on the first grant. Each read pins that same folder
  for the duration of the read. It checks the opened file's type by bytes
  before issuing a URL and on every request. File names are literal, with no URL or line-number parsing. The visual
  reader refuses protected paths, links (including hard links), reparse points, devices and traversal. A replacement
  regular media file can be read in a new request. A missing, unsafe or non-media replacement refuses the request.
- A player gets a fresh URL for each load, including after it was unloaded off screen. It clears its element's
  source and calls `media_release({ lease, url })` to drop that grant independently. Chromium can keep already
  delivered bytes in its in-memory media cache, so a player never reattaches a previously issued URL.
- Only the screen's media subresources use this transport. Electron uses `faber-media://app`, Tauri uses
  `faber-media://localhost` or its Windows alias `http://faber-media.localhost`. Each screen CSP has only its own
  origin for images and audio/video. SVG uses an image content type and an offline sandbox response policy.
  Responses have a fixed media type, no sniffing, no CORS grant and no caching. The visual pane keeps its own CSP.
- GET byte requests support one bounded range, including open-ended and suffix ranges. A chunk carries at most
  1 MiB with a 206 and Content-Range. Unranged images keep the existing approximate size limit (8 MiB). Grants
  retain only a path and an opaque token, up to 512 paths in the one active view. No process, watcher or idle loop
  is added. Leaving the view drops the grants. The screen releases its element's source.
- A refused grant returns `error.media.path` (outside or protected), `error.media.closed` (inactive view),
  `error.media.gone` (missing, unsafe or wrong bytes), or `error.media.limit` (capacity reached). Protocol
  refusals include the key in X-Faber-Media-Error. An element error can re-request the grant to detect a missing
  replacement. A malformed or unsatisfiable range returns 416 and `error.media.range`.

## Web media in a conversation

- Adjacent web pictures share the reply gallery, including hard breaks and picture-only lists. URLs can be extensionless.
  A Markdown image in a sentence previews below its paragraph. Ordinary links stay links.
  A bare media URL after a caption can preview.
- `media_grant_web({ id, url })` uses the visible conversation's lease. Live loads direct media automatically.
  Off and Cached refuse before DNS. Cached uses saved search results. Release or leave revokes the grant.
  Each request reads the current setting.
- Replies keep the source links. Off shows one muted notice per reply and points to Live under Settings ›
  General › Web search. Further Off items are plain titled links. Photo-only lists use normal list spacing.
- HTTPS hosts and subdomains must pass Privacy checks before DNS and connection. All resolved addresses must be
  public. There is no DNS fallback. TLS uses system roots without fetching certificate URLs. Redirects repeat
  the checks. Requests send the client version, project URL and range, without proxy, cookies, credentials or
  referrer. The owner accepted that hostile media addresses can encode conversation data.
- The host allows three redirects, five seconds to connect and fifteen seconds overall. Two fetches run at once,
  with up to 62 waiting. A known length and an initial probe establish size and type. An opening range can supply
  the 4 KiB probe. Pictures are capped at 8 MiB, audio and video at 128 MiB with up to 1 MiB per range.
  Larger audio and video require range support. Scoped responses have a fixed type, no sniffing and no caching.
  SVG is served whole under the image sandbox. Only the address and token are kept for later requests.
- Refusals distinguish `error.media.webOff`, `error.media.webCached`, `error.media.webBlocked`,
  `error.media.webHttps`, `error.media.webPrivate`, `error.media.webSize`, `error.media.webKind` and
  `error.media.webUnreachable`. Scope, capacity and malformed range still use the local media keys.
- A video on a web page uses the built-in browser beside the conversation. Only the video card and one reply-link
  menu action offer this exception on Windows in Chat and Projects. Untitled cards name the video site and keep
  the original link. Video cards send no request before Play. Duration stays in the player controls, with a dark
  surface behind the decoded frame.

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
  and applies the same memory cap and 15 MiB post-close margin after collection. The separate visual_answers_memory
  scenario allows 25 MiB over the cold baseline for the first close after twenty switches (browser cache warm-up), then
  5 MiB over that warmed reading for the 720 px reopen, twenty more switches and Escape; the 400 MB cap and frame/process checks stay.
  Only FABER_LIVE_CDN=1 enables this
  real-CDN step; otherwise it prints that Mermaid Live was not measured. Offline HTML always runs. The opted-in
  check requires internet and fails if drawing falls back to code. Task 310 is complete and parity M1 is implemented.
