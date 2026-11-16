# A blocked account or a restart never leaves a conversation blocked or asleep

Area: project
Files: engine-core and engine-projects (waits, account and sign-in state), the shell restart path in `electron/` and `src-tauri/`, `docs/product-map.md`
Depends: 400-every-wait-has-a-wake
Size: one pull request

Why: The session that studies these autonomy failures (conversation 48cf624a, 2026-11-15) ran a real test with a cheap model and reported two things that may still be open when this task starts: when the time limit passes before Faber confirms the account, the conversation stays blocked even after the sign-in is confirmed later; and its test of restarting Faber with saved waits had not passed yet. A person who sleeps while Faber updates or crashes must find the waits still there and woken. Read that conversation's final report and the commits it landed first: remove from this brief anything already fixed and prove the rest.

Build: Waits and wake events are stored so that quitting or crashing Faber and starting it again keeps every pending wait and fires the ones that came due in the meantime. A conversation held because the account could not be confirmed is released when the confirmation arrives later, and shows the reason while it waits.

Leave out: storing any CLI credential or token (never read or copy them); changing how people sign in.

Done: A test fails without the change: a saved wait whose time passed while Faber was closed wakes at start; a conversation held for an unconfirmed account is released when it is confirmed. A real restart run with the cheapest models shows a waiting conversation surviving it. The commit body states the memory cost.
