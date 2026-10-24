# A visible way out of narrow navigation

Status: local behavior and independent visual review passed; integration pending.

## Observed problem and scope

Both bounded newcomer walks in a 720 px window stalled with Projects navigation open over a blurred work area.
The Projects page existed; closing the drawer exposed it. Escape and the labelled scrim worked, but the scrim was
outside normal Tab order and no close control was visible inside the drawer. The existing decision to keep navigation
open while switching Chat/Projects remains unchanged.

The shell now supplies its existing close action to the sidebar only at narrow widths. A visible close icon sits beside
New conversation, reusing `side.hide`, the ordinary icon button and glyph. Closing returns focus to the original menu.
Docked navigation retains its full-width New conversation button and has no overlay close control. No setting,
shortcut, translation key, stored value, animation or additional dismissal state was introduced.

## Regression and behavior evidence

The new focused native check failed before implementation: English empty Projects had no explicit close control,
0 passed / 1 failed, natural exit 1. The run took 1543.2 seconds including the ordinary full-screen hold; build took
100.6 seconds and the UI step 1.9 seconds. Its log and screenshots were retained before the next run.

After implementation, the `narrow navigation` filter passed both checks, 2 passed / 0 failed, natural exit 0. Total
297.2 seconds including the full-screen hold, build 31.3 seconds, UI 5.6 seconds. The new check covers all five
languages at 720 px: switching to empty Projects keeps navigation open; close is visible without hover, has its
translated accessible name, belongs to normal Tab order, fits beside New conversation inside the window, closes the
drawer and returns focus. At 1280 px it is absent. The existing check still covers area switching, row selection,
Escape, Catalog dismissal and the modal keyboard contract.

Seven lightweight checks passed: lint, TypeScript, documentation, production web build, Rust formatting, script checks
(139 passed, 1 skipped), and lane claims. Existing lint and bundle-size warnings remain. A fresh read-only review found
no code blocker; cards checked: C006 ok, C007 ok, C011 ok, C005 preserved.

## Native visual evidence and identities

Evidence lives outside disposable caches in `faber-evidence/sidebar-close-20261024/`. The before capture used the actual
unchanged old build; the after capture used the actual corrected build, with no style planting. The paired receipts
record source revision, stylesheet, actual app and driver digests. Portuguese captures cover 720×480 and 1280×800;
German after captures repeat those sizes. Each narrow walk closes navigation and verifies menu focus and exposed work.
Both after drivers and the before driver exited 0, printed CLEAN and retained an unchanged app binary.

| Artifact | SHA-256 |
| --- | --- |
| Before app | `c577eb0c8268779673c64c0151d598030c81ce4db6f4bcb8bbfe9e21dbd6f04e` |
| After app, both languages | `4b54472d85da2f6ca68a2d844c0b3a8519b2e90469844e0cac5c009a83483cf0` |
| Native audit driver | `660f6e3fbc0f737c33fe4a113da13da4c4bbdc5b737cf67025ade39c1d8b15c2` |

At 720 px the close target is 32×32 px at (244, 48), opacity 1, with an 8 px gap after the 224 px New conversation
button. Main inspection found the close visible and no overlap in either captured language. German uses the existing
ellipsis for its longer New conversation text. Fresh independent visual scoring found no blocker: paired Portuguese minimum 39/40 before and 40/40 after; including German after, the observed after minimum is 39/40 because of the truncated primary label. No German before image was captured, so no comparative claim is made for that language. Both closed views scored 40/40.

## Cost and limits

The change adds one header wrapper and a narrow-only finite icon button, with existing event handling. It adds no
background task, timer, observer or persistent state. Runtime processor and memory cost was not measured locally;
the integrated memory proof remains required. The first-user matrix and full motion inventory are unfinished.
No integrated gate, stable release or completed roadmap task is claimed by this local proof.
