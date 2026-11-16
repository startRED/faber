# Verify finite waits after installation

Area: project
Files: installed app verification with isolated data, engine waits, UI wait status
Depends: none
Size: one pull request

The finite deadline, restart restoration and real GitHub state checks were delivered. Isolated regression tests, a real GitHub read and a real AI restart run passed. The app in use was still the older installed version and saved a note-only wait without an `until` value; this is not a confirmed defect in the delivered code.

Done: After the person updates the installed app, verify in an isolated test that a note-only wait has a finite saved deadline and wakes or visibly reports its blocker without a person. Do not touch active conversations or the person's data. Investigate and fix only if the updated app fails this proof. Additional stuck presentation and recovery actions remain in 420. See [the diagnosis](../dev/autonomy.md).
