# Icons and marks

One picture for each kind of step and each state, so a screen reads with little reading. The words stay: a drawing is
decoration beside them and never replaces them. The code is `ui/Glyphs.tsx` (the drawings) and `ui/icons/kinds.tsx` (which
drawing means what). Decided 2026-10-31 in the rework, by the owner.

## One set, one rule

- Every drawing is a few paths on a 24 px grid in `ui/Glyphs.tsx`, stroked in the colour of the text beside it (`svg.i`,
  1.75 px, round ends). No fills, no hex colour, no second icon set on one surface. The drawings are original to Faber.
- A person who cannot tell the colours apart still tells the states apart: each state has its own silhouette. A check
  (`tests/ui/icons.check.ts`) fails when two states share a drawing.
- A screen never picks a drawing by itself for a step or a state. It asks `toolKind(name)` and `ToolGlyph`, `stateKind(word)`
  and `StateGlyph`, `ActorMark` in `ui/icons/kinds.tsx`, so the same thing looks the same everywhere.

## Steps

| Kind | Drawing | Names it stands for |
| :--- | :--- | :--- |
| Read | an eye | `Read` |
| Edit | a pencil | `Edit`, `MultiEdit`, `NotebookEdit`, `fileChange` |
| Write | a page with a plus | `Write` |
| Run | a prompt | `Bash`, `PowerShell`, `commandExecution` |
| Search | a magnifier | `Glob`, `Grep`, `ToolSearch` |
| Web | a globe | `WebSearch`, `WebFetch`, `Browser`, `webSearch` |
| Subagent | one node over two | `Agent`, `Task` |
| Helper AI | two rings that overlap | `Helper` |
| Faber's decision | a stem that forks into two arrows | what Faber chose or sent on its own |
| Plan, question, skill, tool server, conversation | list, speech bubble with "?", wand, plug, speech bubble | `TodoWrite`, `AskUserQuestion`, `Skill`, `mcpToolCall` and `mcp__*`, `Conversation` |
| Thought | a feather | thinking within an answer |
| Anything else | a bolt | a name nobody mapped never gets a wrong picture |

## States

| State | Drawing | Ink | Moves |
| :--- | :--- | :--- | :--- |
| Working | an open ring | accent | breathes (`pulse`, `--dur-pulse`); the open ring is the cue without motion |
| Waiting for you | the speech bubble with "?" | amber (locked: needs you) | no |
| Stuck (works and gets nowhere) | an hourglass | amber | no |
| Concluded (the AI gave its conclusion) | a ring with a check | full ink | no |
| Ended without a conclusion | a broken ring with a dash | second ink | no |
| Interrupted (Faber closed or crashed mid-turn) | a ring with a pause | second ink | no |
| Stopped by you | a ring with a square | third ink | no |
| Failed | a warning triangle | danger | no |

`stateKind(word)` turns the engine's words (`running`, `done`, `passed`, `unreported`, `closed`...) into one of these; a word
nobody mapped is "ended without a conclusion", which never claims anything good. A drawing with no state words beside it
takes a `label`, and then it is an image with that name; beside the words it is hidden from readers. Reduced and no motion
stop the breathing (`.st-working` has its own rule in `ui/icons/icons.css`, which holds the state colours too).

## Who is speaking

`ActorMark` and `VendorMark` draw a shape and a colour, never a vendor's logo and never a vendor's name: a coral square for
Claude, a violet diamond for Codex, Faber's own mark (Curioso, in the second ink) when Faber, not an AI, said or chose it.
The three meaning colours stay locked ([looks.md](looks.md#meaning-colours-are-locked)); the shape comes with the colour
and the name always comes with the mark.

## Where it is used

`Details.tsx` (a helper's or a command's state in the Technical details list) uses the state drawings from this change. The work log uses the shared step drawings and state marks, including helpers, thinking and end rows. The running column and sidebar adopt them in their own lanes.
