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

Seen live on 2026-10-29: Settings shows "Os projetos rodam os meus próprios hooks da CLI", "Em cache" for web search,
"Sem cartões: o computador inteiro", "Faber hoje: US$ 9,46 em valor de API" (read as a bill), "janela de 5 horas ≥ 2%"
(the "≥" and the bar's tick mark are not explained), and the access menu lists "Perguntar / Aceitar edições / Planejar
/ Automático / Acesso total" with no line saying what each allows ("Planejar" is not an access level). The same actor is
called "IA", "assistente", "agente", "Claude" and "Faber" on one screen. The full lists are in the three judges'
reports kept with the diagnosis archive.
