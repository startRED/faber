# Thread list: needs you first, calm status

From the T3 Code study (`02-agent-progress.md`). One status per thread: needs the person > running > done, unread dot,
helper threads under their parent, a collapsed project shows its loudest status. Anti-flicker: keep "working" 2 s after
it stops, delay "Done" 2 s, background work blocks "Done". A helper's approval shows as one banner in the parent.
Notifications remain limited to requests that need the person (R74), deduplicated by request; never notify merely
because a run starts or finishes. First sightings, archived threads and runs the person stopped stay silent.
Check a run completing after its thread is archived: it must not notify.
Extends: product-map entry for the running column and notifications. Check cost: no new process, no polling.

Inventory, 2026-10-30: reuse `engine/src/engine.rs` (`running_items`, `running_item`) for needs-you priority;
`ui/Sidebar.tsx` and `ui/sidebar.ts` (`tailOf`) already provide collapsible projects and needs-you-over-running
aggregation. Do not add a second project tree. `ui/Running.tsx` is the active-work column, not the history list.
`ui/Thread.tsx`, `ui/AgentChat.tsx` and `ui/worklog.ts` already show helpers and their details. Extend these rows.
`src-tauri/src/tray/decide.rs` already deduplicates question notices; preserve that stronger per-question behavior.

On the refreshed base, `engine/src/engine/ended.rs`, `Summary.ended`, `ui/running.ts` (`doneCards`) and
`ui/Sidebar.tsx` already provide persistent unread completion markers and finished cards; tests include
`tests/ui/donecard.check.ts`. Preserve stopped/archived suppression, clearing on read/new run and restoration after restart.
Remaining behavior to prove before implementation: a calm finished
state with a bounded 2 s settle timer (no interval, immediately superseded by a question or new run); no finished
state while a background helper is live; one parent approval surface using the existing permission cards. First
inspect the background-helper lifecycle and archive/remove semantics: neither a missing running item nor an old
helper result alone proves successful completion. Never delay a permission card for anti-flicker.

Acceptance: deterministic clock checks cover fast stop/start, late helper completion, a new question during the
settle delay, stopped/failed turns, navigation/read acknowledgement and reload. UI checks cover both areas, a folded
project, keyboard navigation, 720 px and reduced motion. Tray checks cover first snapshot, stopped turn, and removal
while a run is active. Record which lifecycle cases already pass; add regression checks only for actual gaps.
