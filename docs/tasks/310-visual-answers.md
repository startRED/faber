# Finish visual answers beside the chat (parity M1)

Step 1 covers closed HTML and SVG blocks in replies: a small card opens the visual in the running column's
resizable pane, with earlier versions kept in the conversation's messages (R311 through R315). Ten built-in pieces are
also implemented inline in replies, with saved controls and plain-code fallback (R348); their checks are in the
product map. Extend the existing reply, pane and card in Chat and Projects; do not introduce a second viewer. Row M1
stays begun until the remaining parts below are done.
Network is delivered (R354): the existing web choice in Settings applies on opening or switching versions, with
Live permitting HTTPS and Off/Cached staying offline. Saving the choice does not reload an open page. The opaque
sandbox, narrow dismissal messages, private-URL filters and one-frame lifecycle remain; the product map records the
WebViews' DNS-filtering limit. There is no per-answer prompt or second setting.
The visual page has its own custom-protocol origin and response-header policy. Faber's own resource policy is the
base policy unchanged (only frame-src names the visual origin), so ordinary replies cannot fetch remote images,
styles, fonts or media. A bounded single-use pending document and pane lease release bytes on delivery, switching
or closing; no file paths or engine data are served. The screen checks cover redirects to private HTTPS and actual
asset/IPC/other-visual-path fetches, blocked private WSS and external script load/error callbacks. WebSocket and
WebTransport stay unavailable on both systems. The Windows callback leaves non-HTTPS responses untouched because
WebView2 shares Wry's custom-protocol filters; macOS blocks visual-origin top-frame loads, and GTK uses its
actual main-frame response policy to preserve Linux offline viewing while refusing visual top documents. Native
Fetch Metadata may be absent; exact single-use URLs and no CORS grant still protect delivery. Resource callbacks
capture image/stylesheet events on Document and deliver them once earlier classic definitions exist; the browser
still controls resource completion order. Scripts preserve authored CORS mode, integrity hashes and safe metadata;
modules still require CORS. Noscript and declarative shadow-root attributes (including nested templates and the
legacy spelling) are removed before serialization. The native parser observer removes nested contexts visible in
the document and watches discovered/explicit closed shadow roots while parsing, then disconnects at DOMContentLoaded.
Native guards now refuse nested document loads independently of cleaning: Windows follows FrameCreated recursively;
macOS blocks other child documents in its rule list; GTK refuses srcdoc/data/blob/external navigations and preserves
shell subresources. Its response API cannot safely distinguish every child document from a subresource. Native navigation APIs cannot stop
initial empty context creation, and those contexts can share the parent's opaque origin. The visual realm locks frame
contentWindow/contentDocument and window.open, and cleans ElementInternals.shadowRoot before exposure; no child script
is needed for these barriers. WebRTC peer/data-channel globals and their webkit aliases stay locked in every web mode. CSP explicitly
denies child frames and workers. Real-frame checks include a namespace-confusion second parse, SVG foreignObject,
MathML, CSS import/url, meta refresh, window.open and workers. A parser-loaded data image, independent of network
timing, proves resource callbacks run before the delayed external classic; network images keep their actual target.
The lead runs those checks and the existing memory/processor scenarios outside the agent sandbox before landing;
no measured cost is inferred from these changes. One frame and one bounded pending document remain the limits.

Land each remaining step on its own, in this order:

1. **Project files and Mermaid.** Detect pages and small apps the agent writes as files in a project, and diagrams
   written as Mermaid. Reuse the reply card, pane and version controls. Define which files count and how file versions
   are retained without reading unrelated project files. Done when fixtures for both CLIs prove detection and version
   retention, and screen checks cover opening each form, isolation, earlier versions, a 720 px window and reduced
   motion. Measure memory and processor cost; update the map, design and five-language what's-new note.
