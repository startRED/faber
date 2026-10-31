# The Look around counter keeps its newest lines instead of stopping at 200 KB

Area: Look around
Files: engine/src/look_log.rs, engine/tests/look.rs
Depends: none
Size: about 100 changed lines

Why: `engine/src/look_log.rs:10`: past its limit "the log stops growing (shortcut: no rotation)". After some weeks the counters that tell whether the Look around offer works freeze, silently, and every later measure reads the old weeks.

Build: when a write would pass `MAX`, drop the oldest half of the file at a line boundary (as the crash file does) and then append, one atomic write like the other small files. Remove the shortcut comment.

Done: an engine test that fails today: fill the log to `MAX` and add a line; the line is in the file, the size is at most `MAX`, the oldest lines are the ones gone, and the counts still add up from what remains. Cost: one rewrite per roughly 1,250 lines.
