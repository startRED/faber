# Say in the access menu what each choice allows

Area: Composer
Files: ui/Composer.tsx, ui/Settings.tsx, ui/styles.css, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/composer.check.ts, tests/ui/settings.check.ts
Depends: none
Size: about 200 changed lines

Why: the live look of 2026-10-29 (`docs/research/2026-10-29-faber-diagnosis/README.md`, part 4 and "Live look"): the
access menu lists "Perguntar / Aceitar edições / Planejar / Automático / Acesso total" and the person cannot tell what
each allows. The lines exist: `set.permissions.<mode>.line` ("Toda alteração espera o seu OK.", "Edições rodam sozinhas;
comandos ainda podem perguntar.", ... `ui/locales/pt-BR.ts` lines 1306 to 1310), but the menu in `ui/Composer.tsx`
(lines 146 to 149) is a native `<select>` that shows them only as a `title` and an `aria-description`, which a native
list does not display, so a person choosing sees five bare names. Settings shows the names with a longer text.

What to build: show the one-line description with each choice while the person chooses, in the composer and in
Settings, through the control the app already has for choices with a line (look in `ui/controls.tsx` and the model and
effort pickers first; extend that control, do not add a second kind of menu). The text is the existing `.line` keys, so
this task changes no words (words are tasks `plain-words-in-settings` and `plain-security-text`). It must work with the
keyboard and a screen reader, at 720 px, and not move the composer.

Leave out: whether "Planejar" belongs in the access menu at all (it is a way of working, not a level of access; that
changes a documented decision, `docs/product-map.md` "Permissions", so it goes to the owner first in a short card
instead), changing which modes exist.

Done: a screen check in `tests/ui/composer.check.ts` fails today because opening the menu shows only five names; it
passes with the line visible under each name, the current choice marked, Esc closing it and focus returning to the
button. Extends: product-map "Composer" and "Permissions". Update `docs/design/composer.md`. What's-new entry in every
language. Walk it at 720 px, longest language, reduced motion, light and dark. Visual critique before and after. No
memory or processor cost. Review by the other vendor (R146) and the coherence review.
