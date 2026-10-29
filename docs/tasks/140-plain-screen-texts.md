# Plain screen texts

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 4. An audit of `ui/locales/pt-BR.ts` on 2026-10-29 found
words a person who does not program cannot read: "galho" (branch), "papel" (role), "cache quente", "CLI", "sessão",
"grafts, clone raso ou alternates", "integridade baixa", "AppContainer", and a 90-word security text in Settings
(`set.permissionsDontAskBody`).

Rewrite them in plain words in English (`en.ts`, the source) and every language, by hand, keeping every warning (a
security text loses no warning; that change gets a security review). One word per idea across the app (the product map
lists the product words): "ajudante" for helpers, never "papel" or "subagente" on screen. Sentences under 25 words.
Search all locale files for the same words first, and read the product map's "Known incoherences" for these screens.
The owner is a native Portuguese speaker: ask him to read the pt-BR diff in a short card before landing. What's-new
entry.
