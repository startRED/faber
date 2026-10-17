# One shared test app for screen checks (R190 V4)

Step V4 of `docs/superpowers/specs/2026-10-17-built-in-verifier.md` together with the shared running app of the test
rework (`docs/superpowers/specs/2026-10-14-test-system-rework-design.md`): the verifier keeps one hidden test app alive
between screen checks, with WebView2's GPU off, requests over stdin, a reset between requests, a restart after N
requests or a memory cap, and an exit after 10 idle minutes. Screen checks stop launching about 6 WebView2 processes
each.

Done when: the screen suite runs through the shared app with the same results; the idle exit and the restart caps have
tests; the run's memory and time before and after are in the commit body and in dev/testing.md.
