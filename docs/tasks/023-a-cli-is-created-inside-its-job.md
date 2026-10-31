# A CLI is created inside its job, so no end of Faber leaves one behind

Area: none
Files: winproc/src/lib.rs, engine/src/proc.rs
Depends: none
Size: about 250 changed lines

Why: on Windows, `spawn_in_job` (`engine/src/proc.rs`) creates each CLI suspended, then creates its job
(`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) and assigns it, then resumes it. When Faber ends between the creation and the
assignment (a crash, Task Manager, a quit while a warm start is under way), the suspended CLI is in no job and stays
forever, paused. Task 011 saw it twice on CI (`queue/pr-474` run 41053322464 and `queue/pr-448` run 41039738430): the
draft attachments scenario ended its first app about 120 ms after one of its CLIs was born, and the end-of-scenario audit
found that `fake-cli.exe` still there (that it was the suspended one is the likely cause, not observed). Task 011 made the scenario wait until the app has started no CLI
for 2 s, which narrows the window but cannot prove a CLI joined its job; the window in the app is still open.

Build: create the process already inside its job, with `PROC_THREAD_ATTRIBUTE_JOB_LIST` (Windows 10+) through
`STARTUPINFOEXW`, in `winproc` (the only crate allowed unsafe code), with the same pipes, no window, priority and
suspended start the engine uses now. std's `raw_attribute` is still unstable, so this means creating the process by hand
and giving the engine its handles. Two reviews (process and handle code).

Done: an engine test ends the creating side between creation and resume (a test hook) and shows no process left; the
draft attachments scenario passes without its wait.
