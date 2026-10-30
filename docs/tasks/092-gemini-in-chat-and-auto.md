# Gemini answers in Chat too, and Auto can pick it there

Area: Auto
Files: engine/src/route.rs, engine/src/engine/auto.rs, engine/src/engine.rs, ui/Composer.tsx, ui/locales/, engine/tests/, tests/ui/, docs/product/models.md, docs/product-map.md
Depends: gemini-profiles-in-routing, gemini-marks-on-screen
Size: about 300 changed lines

Why: the owner chose (2026-10-30) that Gemini (through Antigravity's CLI) works in Chat as well as in Projects, including as
an Auto pick there. The tasks 055 to 090 build it for Projects only.

Build: a Chat conversation can run on Gemini through the same `agy` turn session (task `agy-turn-session`), the model menu
offers it when `agy` is connected (extend the existing menu, no second one), and Auto's Chat candidates include the Gemini
profiles of task `gemini-profiles-in-routing`, weighed by the same quota pace. Chat keeps its own tool set (no project tools).

Leave out: anything Projects already has; new profiles or measurements.

Done: engine tests that fail today: a Chat turn routed to Gemini runs through `agy` (recorded run), Auto in Chat picks a
Gemini profile when the others are behind pace, and a Chat conversation never gets project tools on Gemini; a screen check of
the Chat model menu with Gemini connected at 720 px in the longest language. Extends: the "Auto" and model menu rows of the
product map. What's-new entry in every language. One review by the other vendor.
