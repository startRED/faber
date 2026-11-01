# Let the person list sites Faber's browser must never open

Area: Page in Faber's browser
Files: engine/src/browser.rs, engine/src/model.rs, engine/src/engine/browse.rs, ui/Settings.tsx, ui/api.ts, ui/locales/, tests/ui/settings.check.ts
Depends: none
Size: about 350 changed lines

Why: `docs/product/parity.md`, row K5: the Claude app lets the person say which sites its browser may or may never open;
Faber's browser opens any http or https page (`checked_url` and the address check in `engine/src/browser.rs` refuse only
other schemes and names that are not sites). The row's way is "a Settings list". A list of sites the agent must never
open is a safety control for non-programmers (a bank, a mail site), and it needs no new process.

Build:

- A `Settings` field with a list of host names (a bounded list, for example 50 entries, each a lowercase host name
  without scheme or path), merged in `Core::save_settings` like the other fields and mirrored in `ui/api.ts`
  (see "Worked example: adding a setting" in the product map; `engine/tests/product_map.rs` stops compiling until the map
  row exists).
- The check lives where the address is validated in `engine/src/browser.rs`, so the agent's `browse` tool, a link the
  person clicks and the app pane all refuse a listed host and its subdomains with an `error.*` key, not a sentence.
  Redirects are checked on every hop the pane reports; if the host cannot see redirects, say so in the map.
- Place the control with `docs/product-rules.md` (a Settings row with one short line and the longer text behind the
  "?", never on a work screen): one text box that adds a host and a list with a remove button each. Words by hand in
  every language.

Leave out: an allow-only list, per-conversation lists, and anything about the pane's host window (the Electron move owns
the pane itself).

Done: engine tests that fail today refuse a listed host, its subdomain and a redirect to it, and open an unlisted one; a
screen check adds and removes a host in Settings; walk it at 720 px and in the longest language. Extends: the
product-map rows "Page in Faber's browser" and "Every setting" (new setting row). What's-new entry in every language.
Memory and processor cost stated in the commit body (a string compare per navigation). One review by the other vendor
(R146), plus the coherence review of a change a person sees.
