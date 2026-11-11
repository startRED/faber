# Team in the reply

A run containing a Faber helper stays in its place among the reply's words and steps (`ui/timeline.ts`), even with
one member. Adjacent CLI subagents join with the metadata known; CLI-only runs keep their nested rows. Review rounds
keep their existing row. There is no setting or extra command. Cards open the existing helper view.

The head reads Team, a decorative segment per member, then the existing working, ended and waiting-question counts.
Each segment takes its member's state: only finished segments are filled in the positive tone; working ones have an
open accent edge. Neutral waiting, amber needs-you and failed segments carry their own state tones. This counts people,
never estimated progress. The segments and the CSS branch are absent from the accessibility tree.

A quiet lead strip shows the reply's own mark, Lead, the known model (`Message.model`, omitted when unknown), and
one real-state sentence: Answering <piece> while a member waits for the lead; otherwise Handed out N pieces while any
member works; after all members end, Handed out N pieces, with answered N questions only for actual recorded lead
answers. Middle dots separate these facts; the sentence uses first ink, stronger than the label and model. These words
and plural forms are hand-written in all five locales. Nothing asks the person to configure it.

One thin horizontal rail starts under the lead's mark and ends exactly at the last card's drop. The grid keeps members on that rail; at a
block width of 460 px or less it stacks, with a left rail and one short stub per card. Borders and empty pseudo-elements
draw it, without measuring positions. Every member owns one decorative drop. Working and background members use the
same quiet --accent-soft on their border and drop; finished cards return to --line. A waiting-lead drop uses neutral ink
until answered; waiting-person and stuck drops use amber. Enlarged text, unbroken titles and models fit 280 and 180 px blocks.

Each card reads: mark and strong two-line piece title; one model · effort line with Read, Build, Review or Advice at its right
end; a state pill; one detail line. The job never shrinks: only the model text ellipsizes, with full metadata in the
accessible name. Pills start lowercase in all five locales, preserving native noun casing. Working members retain
their observed activity; waiting members show their current recorded lead question or pending AskUserQuestion text.
Ended members with reports show their first sentence as one plain-text outcome, with the known duration at its right
end. All these details ellipsize to one line. Missing activity, question or report produces no placeholder. Cards in a
row share its natural height, without a fixed minimum. The existing
safe Markdown reader strips formatting; the language's native sentence segmenter preserves decimals and punctuation.

The existing state palette supplies every tone: positive is --ink (the concluded check's existing full ink), working
is --accent-text; waiting for the lead is --ink-2 on --s3, with --ink-3 on its drop; needs-you is --amber-ink with
--amber-line on --s2; failed is --danger on --s2; stopped/declined use --ink-3 on --s2. Other pills use --s3. Chips use
--s2 with --line-2: Question and Note use --ink-2, Answer --ink, No answer --ink-3. No new colour, glow or gradient.
Cards use --s2, --line and --r-md; the quiet lead strip uses --s1, --line and --r-sm. Actor marks keep their locked inks.

Messages show a typed Question, Answer, Note or No answer chip, visible sender → recipient by piece title or Lead,
then the text in the same flow. Names use second ink, text first ink; text follows names when it fits and wraps naturally
at every width. Collapsed exchanges clamp to two lines; expanding the native button shows both names and text in full. Answers and recorded
no-answers stay nested under their question. A lapsed question without a recorded no-answer has one named fallback.
Mirrored tells show once. Only the newest three exchanges mount until Earlier exchanges is opened; answers stay with
their question. The engine's stored message limit is unchanged.

When both reply and every member end, all details leave the DOM. The caret and check receipt reads Team finished ·
N pieces · N messages · time, preserving named failure/stopped warnings. Opening shows a delivery list: mark and title
with duration and a check on the same line, model · effort, then the outcome below. Successful deliveries show only the
check glyph, accessibly named finished; failed, stopped and declined members retain their worded pill. At narrow widths
those exceptional pills take their own row, preserving the title, duration and full state. Then Messages (N) and the
exchanges follow. No lead strip or branch remains.
Background members keep the team open. Folding returns focus from a removed member to the receipt, preserving newer
focus outside the team. Cards keep their identity during streaming. Keyboard order is cards, earlier disclosure,
message buttons and their answers; the lead strip has no action and cannot take focus.

Rails, drops, cards and new message lines arrive once with --dur-arrive and --ease. A newly recorded message replaces
one soft border ring on its sender (or lead strip). Unchanged message ids never retrigger it. Only the existing working
mark loops. Every team animation stops under system reduced motion, app reduced/none, or a completed receipt; hidden
and quiet windows retain the shared pause rule. No timer, observer, position script or animation listener is added.

A hidden Electron fixture with three members and two exchanges measures 80 elements working, 70 ended-open and 7
folded, including the team root, against the second look's 67 / 65 / 7 and first version's 60 / 70 / 5. Working includes
real observed tool activity and the waiting member's actual question; ended-open includes three real one-line
reports and durations. Model/detail wrappers, inline exchange bodies and lead separators account for the extra nodes;
finished deliveries remove the repeated word span. Sender rings add at most one element per sender and replace themselves on a new message id.
The bounded existing messages feed derived values; no new process or recurring screen clock belongs to this block.
Physical RAM/CPU have not been measured. Captures are synthetic; installed-app and native screen-reader checks remain
outside these proofs.

`tests/ui/team.check.ts` covers states, lead sentences, names/chips, mounting, stable identity, motion, rendered contrast
in every theme/preset, and the five locales at 280/180 px with text above the app's largest size. Existing transition
announcements in `Thread.tsx` stay individually atomic and occur once per transition, independent of check tallies.
Fields: [helper channel](helper-team.md). Existing member view and CLI rows: [subagents](subagents.md).
