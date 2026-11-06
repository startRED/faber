# TL-ADE pipeline map: full translation and lessons for Faber

Source: Erick's artifact "Mapa da TL-ADE" (https://claude.ai/artifact/Mgb6DwipBeQuyZ5UszFSEv), read in full (all 747
lines of its source) on 2026-10-12. It shows the TL-ADE as it was meant to be when complete: 26 fixed lines ("metro
lines"), each a list of stations. Only the Software line was being built.

This file is a faithful English translation of every line and station, kept so Faber's design can use the map without
the original page. Wording is translated, not summarised; nothing was dropped.

## Page frame

- **Subtitle:** each request rides one line, station by station, until it is done.
- **Plain chat vs TL-ADE panel.** Plain chat: you → one AI makes it and approves itself → answer. TL-ADE: you →
  understands → research (program) → the team makes it (AI, Blender, program) → another AI checks, and if it fails the
  work loops back and is redone → delivery.
- **Six gains shown:** asks first; another AI checks; redoes until it passes; changes only the right part; copies what
  worked; continues where it stopped.
- **Who does it (legend):** You; AI; program alone; program + AI; AI inside an app (Blender, Photoshop, Premiere);
  "goes back and redoes" (dashed arc).
- **Levels.** Level 1, simple ("one tool, a few minutes"): quick question, translation, PC tasks, learning, title, edit
  image, new image, audio and voice, animated video. Level 2, complete ("several steps and tools"): research, ideas,
  purchase and price, text and script, data and spreadsheets, documents and slides, video idea, thumbnail from scratch,
  video editing, 3D model, brand and logo, software. Level 3, combinations ("join several lines in one delivery"): full
  YouTube video, marketing campaign, product launch, data to presentation, client website.
- **Footer (build order):** "This is the plan. Today the TL-ADE builds the Software line; next come the questions about
  what the program is for, and the Client website; then the Thumbnail from scratch. The others come later, one at a
  time, each tested before release."
- **Default "Understand the request" station** (used where a line does not define its own): asks only the questions
  that change the result; if the request is already clear, goes straight on.
- **Default "After delivery"** (used where a line does not define its own): ask or request more in the same
  conversation; it reuses what it already made and redoes only what changed, without you repeating yourself.

Notation below: `[You]`, `[AI]`, `[Program]`, `[Program+AI]`, `[Blender]`, `[Photoshop]`, `[Premiere]`. `→ line` means
the station runs another line (a hub). `↺ n→k` is a back edge from station n to station k, with its reason.

## Level 1 · Simple

### Quick question (Text and research)
Example: "which graphics card is the best value today?"
Why better than a chat: every piece of information comes with where it came from. Nothing invented.
1. [You] Your question — any doubt.
2. [AI] Thinks — decides whether it needs to search the web.
3. [Program] Opens the sites — opens the real pages and stores them.
4. [Program+AI] Finds the proof — ties each piece of information to the passage of the site that proves it.
5. [AI] Answers — writes a short, direct answer.
6. [Program] Checks — verifies the passage really exists and the math adds up.
7. [You] You read — the answer with the links.
- ↺ 6→4: information without proof? Goes back to searching.
- After delivery: asked "and is this other one better?" It answers with what it already found and only searches for
  what is missing.

### Translation (Text and research)
Example: "translate the video description into English"
Why better: numbers, names and links never change. And the text is read where it will appear.
1. [You] Your request — the text or the file.
2. [AI] Understands the request (default).
3. [Program] Protects — separates the text and protects names, numbers and links.
4. [AI] Translates — adapting jokes and expressions, not word by word.
5. [Program] Checks — numbers, names and length are still right.
6. [AI] Another AI checks — an AI that speaks both languages reads it in the place where the text will live.
7. [Program] Returns — the translation in the same file.
- ↺ 6→4: sounds odd? Goes back to translating that part.

### PC tasks (Work and business)
Example: "organise my downloads folder"
Why better: logs every action and never does something twice because it thought it failed.
1. [You] Your request — tidy files, fill a form or work on a website.
2. [AI] Understands the request (default).
3. [Program] Looks at the current state — how the folders or the page are now.
4. [AI] Plans — decides the steps, one by one.
5. [Program] Does it — does each step and logs what it did.
6. [Program] Checks — looks again to see it came out right.
7. [You] Summary — says what it did and what it did not do. A purchase or a send is left ready, waiting for you.
- ↺ 6→5: not right? Redoes only that step.

### Learning (Work and business)
Example: "teach me to use Blender"
Why better: exercises with grading, not just explanation.
1. [You] Your request — what you want to learn.
2. [AI] Understands the request (default).
3. [AI] Study plan — starts from what you already know.
4. [AI] Explains — with examples.
5. [AI] Exercise — gives exercises to practise.
6. [You] You practise — in your own time. If you do not answer, the material stays ready for later.
7. [Program+AI] Grades your answers — corrects and shows where you went wrong.
- ↺ 7→4: wrong? Explains that part again.

### Title (Video and YouTube)
Example: "a title for the Return of Death video"
Why better: the title is designed together with the thumbnail and tested among real videos.
1. [You] Your request — you describe the video idea.
2. [AI] Understands the request (default).
3. [Program+AI] Looks at titles that worked — gathers titles that did very well and very badly and finds what they
   have in common.
4. [AI] Finds the hook — what makes a person want to watch to the end.
5. [AI] Writes titles — 10 titles written together with the thumbnail, never repeating what the image already shows.
6. [Program+AI] Tests on a phone — puts each title with the thumbnail among real videos, at phone size.
7. [AI] Another AI checks — removes titles that are confusing, exaggerated or promise what the video does not have.
8. [You] You choose — receives the best 3, each with its reason. If you do not choose, the first one stays.
- ↺ 7→5: no good title? Goes back to writing.

### Edit image (Image, brand and 3D)
Example: "make the tears in this thumbnail look better"
Why better: a chat says "improved" even when it got worse. Here the edit only stays if it beats the original.
1. [You] Your request — you send the image and say what to change.
2. [AI] Understands the request — separates what may change from what must not be touched, such as face, logo and text.
3. [AI] Plans — how to change it without spoiling the rest.
4. [Photoshop] Edits — in Photoshop, in layers, so it can be undone.
5. [Program] Checks — what was not supposed to change stayed the same.
6. [AI] Compares without knowing which is which — two AIs look at the new and the original mixed up and pick the
   better one.
7. [Program] Delivers — shows before and after and delivers the file.
- ↺ 5→4: touched where it must not? Goes back to editing.
- ↺ 6→3: the original won? The original stays, or it tries another way.

### New image (Image, brand and 3D)
Example: "an ad for my father's shop"
Why better: makes 3 quick versions before polishing one. And the text never comes out warped.
1. [You] Your request — what the image shows and where it will be used.
2. [AI] Understands the request (default).
3. [AI] 3 drafts — 3 quick, very different versions.
4. [Program+AI] Chooses — sees which works best at the size it will appear. The best goes on by itself.
5. [Photoshop] Polished — the final version in Photoshop. The text is set separately.
6. [AI] Another AI checks — looks closely for plastic skin, odd edges or warped text.
7. [Program] Delivers — the image and the editable file.
8. [You] Your opinion — the image arrives finished. If you want, say what you think, ask for a change or swap to
   another draft. Your opinion is saved for the next ones.
- ↺ 6→5: found a defect? Goes back and fixes it.
- ↺ 8→5: asked for a change? Redoes only the final version. The drafts stay saved in case you want to swap.

### Audio and voice (Sound)
Example: "clean the noise out of my recording"
Why better: the AI only says it sounds good if it really listened. If it cannot listen, it says so.
1. [You] Your request — clean a recording, podcast, narration, or join voice and music.
2. [AI] Understands the request — asks where the audio will be used and whether you want an AI voice.
3. [Program] Listens to the numbers — measures noise, too-loud volume and silences.
4. [AI] Plans — decides what to do in each part.
5. [Program] Cleans and mixes — removes the noise and sets the volume, without deleting the original.
6. [AI] Voice, if asked — only if you asked: narration with an AI voice you already approved.
7. [AI] Another AI listens — really listens and finds a strange voice or music too loud. If it cannot listen, it says
   so at delivery.
8. [Program] Delivers — the final audio and each track separately.
- ↺ 7→4: bad sound? Goes back to planning.

### Animated video (Video and YouTube)
Example: "a 30-second video explaining the TL-ADE"
Why better: each scene is made separately. Changing one scene does not spoil the others.
1. [You] Your request — what the video should show.
2. [AI] Understands the request (default).
3. [AI] Short script — splits into scenes: what appears and what is said in each.
4. [AI] Builds the scenes — creates each animated scene, the HyperFrames way.
5. [Program] Voice and music — places narration and music on time.
6. [Program] Takes stills — stills of the key scenes, for checking.
7. [AI] Another AI checks — watches and finds a confusing or slow scene, or text that disappears too fast.
8. [Program] Becomes a video — joins everything and renders the video.
9. [Program] Delivers — the video and the scenes for later edits.
10. [You] Your opinion — the video arrives finished. If you want, say what you think or ask for a change in one scene.
    Your opinion is saved for the next ones.
- ↺ 7→4: bad scene? Goes back to that scene only.
- ↺ 10→4: asked for a change in one scene? Redoes only that scene.

## Level 2 · Complete

### Research (Text and research)
Example: "what is the best way to make money with a small channel?"
Why better: two sites copying the same story count as one. And the answer never claims more than the sources show.
1. [You] Your request — a big question.
2. [AI] Understands the request (default).
3. [AI] Splits — breaks the question into smaller parts.
4. [Program+AI] Searches and reads — reads sites, PDFs and tables and stores each source.
5. [AI] Notes the facts — for each fact: who confirms it, who disagrees and from when.
6. [AI] Writes the answer — joins everything into one answer.
7. [AI] Another AI checks — reads the answer and points out exaggeration or a misused source.
8. [Program] Report — the answer with all the sources.
- ↺ 7→6: exaggerated? Goes back to writing the answer.

### Ideas (Text and research)
Example: "give me name ideas for my father's shop"
Why better: several AIs think separately, without copying each other. Each idea comes with what already exists that is
similar, with a link, and with its risk.
1. [You] Your request — a name, product, business, theme or new feature.
2. [AI] Understands the request — who it is for, what is off-limits and what counts as a good idea. If you already have
   ideas, you write them before seeing the AI's.
3. [Program] Looks at what exists — searches similar products, names and channels.
4. [AI] Several AIs, each alone — 6 to 12 AIs from different companies, without seeing what the others made. Each
   thinks a different way: borrows from another field, inverts the problem, takes the customer's place, or works under
   a tight rule.
5. [Program] Merges the repeats — near-identical ideas become one.
6. [Program+AI] Does it exist? What if it fails? — for each finalist, really searches for the similar thing that exists
   and shows the link. Imagines the idea failed and writes down why.
7. [AI] Tournament — AIs from another company compare ideas two by two, swapping the order so the first is not
   favoured. The result is a short list, not a single winner.
8. [You] You choose — 3 to 5 ideas, each with its existing lookalike and risk. If you do not choose, the first goes on.
9. [AI] Cheap draft → New image — when the idea calls for it, it gets a quick test: a logo, an image or a page.
- ↺ 5→4: few distinct ideas left? Asks for another round, different from the ones already produced.
- ↺ 6→4: everything already exists? Generates again, borrowing from distant fields.

### Purchase and price (Text and research)
Example: "build a PC up to R$ 1,200 for Blender"
Why better: reads the whole description and the seller's history before recommending. In the real test, a "cheap"
graphics card had 6 defects written only at the end of its description.
1. [You] Your request — what you want to buy and how much you can spend.
2. [AI] Understands the request — at most 3 questions: is used OK? Is a warranty needed? What is your postcode?
3. [Program] Probes prices — looks at a price comparator before anything else. If the money cannot buy new, it warns
   right away.
4. [AI] Checks what fits together — decides the parts and what does not work together, using tables, not memory.
5. [Program] Sweeps the listings — goes through hundreds of listings and drops those without warranty, too expensive or
   with a defect in the title.
6. [Program] Opens the finalists — reads the whole description, the reviews and each seller's history.
7. [AI] Looks at the photos — checks the part in the photo really is the one in the listing.
8. [Program] Total cost — adds shipping, tax and what is still missing to buy.
9. [AI] Builds 3 options — the cheapest reliable one, the best value, and one that stretches a bit. Says what it
   discarded and why.
10. [AI] Another AI checks — checks everything fits together, redoes the math and checks the warranty and tax rules.
11. [You] You read — the 3 options with links and what to check before paying. The purchase is always yours.
- ↺ 10→6: dubious seller or a part that does not fit? Leaves the list and goes back to the finalists.
- After delivery: remembered later that "it must have a warranty"? It re-filters what it already found before
  searching more.

### Text and script (Text and research)
Example: "a 10-minute script for the video"
Why better: a second AI removes the "written by AI" feel: identical lists, empty sentences and everything in the same
rhythm.
1. [You] Your request — what to write and for whom.
2. [AI] Understands the request (default).
3. [AI] Learns your voice — reads your texts to write your way.
4. [AI] Organises — tests 3 different openings and keeps the best.
5. [AI] Writes — writes the whole text.
6. [Program] Checks — names, facts, length and the parts that could not change.
7. [AI] Another AI checks — removes robot sentences and empty promises.
8. [You] You read — the finished text.
- ↺ 7→5: odd sentence? Goes back to rewriting only that part.

### Data and spreadsheets (Work and business)
Example: "how much did the shop sell per month this year?"
Why better: the math is done by a program and redone to check. The AI never does math in its head.
1. [You] Your request — the spreadsheet and the question.
2. [AI] Understands the request (default).
3. [Program] Checks the spreadsheet — finds repeated rows, empty cells and wrong data.
4. [AI] Plans — decides which calculation answers your question.
5. [Program] Calculates — does the math.
6. [AI] Explains — says what the numbers show.
7. [Program] Redoes the math — calculates everything again to be sure.
8. [AI] Another AI checks — whether the conclusion makes sense or exaggerates.
9. [Program] Delivers — spreadsheet, charts and the answer.
- ↺ 8→4: wrong math? Goes back to planning.

### Documents and slides (Work and business)
Example: "a 10-slide presentation about the project"
Why better: every page is seen as an image before delivery. No text spilling out of its box.
1. [You] Your request — the subject and who it is for.
2. [AI] Understands the request (default).
3. [AI] Organises — decides what goes on each page or slide.
4. [AI] Content — writes the texts and looks up the numbers.
5. [Program] Assembles — puts everything into the document template.
6. [Program] Takes pictures — a picture of every page.
7. [AI] Another AI checks — whether it is clear; finds text spilling out of a box or a chart that says nothing.
8. [Program] Delivers — the editable file.
- ↺ 7→5: ugly page? Goes back to assembling.

### Video idea (Video and YouTube)
Example: "what should I record for my Minecraft channel?"
Why better: a chat invents ideas. Here ideas come from what really worked in your niche.
1. [You] Your request — the theme or the channel.
2. [AI] Understands the request (default).
3. [Program] Gathers videos — collects the numbers of hundreds of videos in the theme.
4. [Program] Finds the ones that blew up — which did far better than the channel's normal.
5. [AI] Finds out why — why each one worked and what nobody has done yet.
6. [AI] Creates ideas — 10 short ideas, and develops the best 3.
7. [AI] Another AI checks — removes copied, dull or hard-to-record ideas.
8. [You] You choose — the recommended idea plus 2 more, each with its reason. If you do not choose, the recommended
   one stays.
- ↺ 7→5: no good idea? Goes back to looking at what worked.

### Thumbnail from scratch (Video and YouTube)
Example: "a Return of Death thumbnail in the Void Append style"
Why better: the idea is tested with a simple drawing before spending time in Blender. The style and the opinions you
give are saved for the next ones.
1. [You] Your request — the theme, your skin and images you like.
2. [AI] Understands the request — asks which moment, which feeling, and whether it is a homage to something.
3. [Program] Gathers examples — thumbnails that blew up in the theme, and ones in the style you want.
4. [AI] Picks the style — camera, light and colour. Proposes 2 or 3 paths.
5. [AI] Idea with the title — what the image shows and what the title completes, without one repeating the other.
6. [AI] Simple drawings — 6 to 10 quick drawings, ugly on purpose, only to show the idea.
7. [Program+AI] Tests the idea — puts the drawings among real videos: very small, can you tell who, against what and
   where? The best goes on by itself, and you can swap.
8. [Blender] 3D draft — builds in Blender matching the approved drawing.
9. [Blender] Builds the scene — character, pose, setting, light and camera.
10. [Program] Final render — Blender renders the picture in high quality.
11. [Photoshop] Retouch — adjusts colour and brightness in Photoshop.
12. [Program+AI] Two AIs check — two AIs from different companies look without knowing who made it: does it follow the
    style, does it tell a story, can it be seen on a phone?
13. [Program] Delivers — the finished thumbnail and the files for later edits.
14. [You] Your opinion — the thumbnail arrives finished. If you want, say what you think or ask for a change. Your
    opinion is saved, and the next one comes out more your way.
- ↺ 7→5: no good idea? Back to the idea with the title. Nothing was spent in Blender yet.
- ↺ 12→9: found a problem? Goes back only to whoever got it wrong: pose goes back to the scene, colour to the retouch.
- ↺ 14→9: asked for a change? Redoes only the owning step and the ones after it: pose goes back to the scene, colour
  to the retouch.
- After delivery: asked for "a redder sky"? Redoes only the retouch. Asked for "another pose"? Redoes the scene, the
  render and the retouch, and keeps the idea.

### Video editing (Video and YouTube)
Example: "turn 2 hours of footage into a 20-minute video"
Why better: if you ask for a change, only that piece is redone, not the whole video.
1. [You] Your request — you send the footage and say what the video is about.
2. [AI] Understands the request — asks whether it is a long video or a Short, whether you want captions and what
   cannot be cut.
3. [Program] Organises — separates the files and makes light copies to work fast.
4. [Program+AI] Watches everything — writes down everything said and notes what appears.
5. [AI] Chooses the cuts — decides what stays and what goes.
6. [Premiere] Assembles — assembles the video in Premiere.
7. [Program+AI] Captions and colour — adds captions and on-screen text and sets the colour.
8. [AI] Another AI checks — watches with sound and finds a bad cut, a slow part or too many effects.
9. [You] You watch — see a preview and can ask, for example, "cut less of the coffee part". If you ask nothing, the
   final video comes out like this.
10. [Program] Finishes — renders the final video and checks it came out right.
- ↺ 8→5: bad cut? Goes back to that piece only.
- ↺ 9→5: asked for a change? Redoes only that stretch.

### 3D model (Image, brand and 3D)
Example: "my mascot in 3D to use in thumbnails"
Why better: you choose the idea from drawings before spending hours in 3D. And a program measures whether the model
matches the drawing.
1. [You] Your request — what you want, with images if you have them.
2. [AI] Understands the request — asks what it is for, the size and whether it will move.
3. [AI] Idea drawings — 6 to 8 quick ideas, no polish, only to see shape and colours.
4. [Program+AI] Picks the idea — an AI checks it can be recognised by its silhouette and fits what you want. You choose
   or the first goes on.
5. [AI] Six-side drawing — the chosen idea from the front, back, both sides and both diagonals, all at the same size.
6. [Program+AI] Checks the sides — draws straight lines across the drawings: eye, shoulder and foot must be at the same
   height in all of them.
7. [Blender] Basic shape — puts the 6 drawings in the background in Blender and builds the shape over them.
8. [Program+AI] Compares with the drawing — renders the model from the same 6 sides and measures whether the silhouette
   matches the drawing.
9. [Blender] Details — adds the details.
10. [Blender] Colour and texture — paints it and gives it materials: skin, metal, cloth.
11. [Blender] Movement — makes it move, if you asked.
12. [Program+AI] Checks from every side — another AI turns the model and looks from every side, still and moving.
13. [Program] Delivers — the finished model, which opens cleanly in the program.
14. [You] Your opinion — the model arrives finished. If you want, say what you think or ask for a change. Your opinion
    is saved for the next ones.
- ↺ 4→3: no good idea? Goes back to drawing ideas.
- ↺ 6→5: one side does not match the others? Redraws only that one.
- ↺ 8→7: the model does not match the drawing? Goes back to the basic shape.
- ↺ 12→9: a melted or strange part? Goes back to whoever made it.
- ↺ 14→7: asked for a change? Redoes only the owning step and the ones after it: shape goes back to the basic shape,
  colour to the texture.

### Brand and logo (Image, brand and 3D)
Example: "a logo for my father's shop"
Why better: the logo is tested small and in one colour, where a weak logo cannot hide.
1. [You] Your request — you tell about the business.
2. [AI] Understands the request (default).
3. [AI] What sets you apart — finds what makes the business different from the others.
4. [AI] 3 styles — proposes 3 very different looks. The best goes on by itself.
5. [Program] Draws the logo — in a file that can grow without blurring.
6. [Program] Versions — colour, black and white, small, business card and social media.
7. [AI] Another AI checks — tests the logo small, in one colour and on several backgrounds.
8. [Program] Delivers — all the brand files.
9. [You] Your opinion — the brand arrives finished. If you want, say what you think or swap to one of the other 2
   styles. Your opinion is saved.
- ↺ 7→4: weak logo? Back to the styles.
- ↺ 9→4: did not like it? Back to the styles, or swap to one of the other 2.

### Software (Software)
Example: "a contact page with a form"
Why better: the test is written before the code. And whoever writes the code never approves their own work.
1. [You] Your request — what the program needs to do.
2. [AI] Asks what it is for — up to 3 easy questions: who will use it, whether it stores other people's information,
   and where it will run. The answer decides which checks apply and is saved in the project. It does not ask what the
   request already says.
3. [Program+AI] Sees what changes — finds the parts of the program that will change.
4. [AI] Test first — writes a test that only passes when the request is done.
5. [AI] Programs — writes the code. Before installing a new library, checks that it really exists.
6. [Program] Quick test — runs the tests for this change and looks for a forgotten password or key in the code.
7. [AI] Another AI checks — another AI, from another company, reads the code and points out errors.
8. [Program+AI] Checks the screen — only when the change shows on screen: opens the real program, takes pictures and
   checks nothing broke, it works by keyboard, and the colours follow the standard.
9. [Program] Full test — runs every test of the program.
10. [Program] Launch check — only when other people will use it over the internet: measures speed on a phone and
    accessibility, tries to see one user's data while logged in as another, and checks Google, broken links and the
    cookie notice.
11. [Program] Saves — stores the change with a receipt.
- ↺ 6→5: test failed? Back to programming.
- ↺ 7→5: found an error? Back to programming.
- ↺ 8→5: screen broken or confusing? Back to programming.
- ↺ 10→5: slow, unsafe or against the law? Back to programming only the part that caused it.
- After delivery: asked for a small tweak, such as changing a text? Takes a fast path, without the interview.

## Level 3 · Combinations

### Full YouTube video (Video and YouTube)
Example: "a video beating Minecraft with the Return of Death"
Why better: each part is made separately. If you change the title, the video edit does not need to be redone.
Joins the lines: Video idea, Title, Text and script, Video editing, Thumbnail from scratch.
1. [You] Your request — you describe the video idea.
2. [AI] Understands the request (default).
3. [AI] Improves the idea → Video idea — makes the idea stronger by looking at what worked in similar videos.
4. [AI] Title and thumbnail → Title — decides the title and the image together, one completing the other.
5. [AI] Script → Text and script — writes what you will say and show, your way.
6. [AI] Shot list — says what to record and in which order.
7. [You] You record — the only part of yours in the middle. With no recording yet, you receive the idea, script and
   list ready. When you send the recording, it continues from here.
8. [Premiere] Editing → Video editing — cuts, assembles and adds captions.
9. [Blender] Thumbnail → Thumbnail from scratch — makes the thumbnail from the idea already approved.
10. [Program+AI] Checks the title — checks the title matches the video that came out.
11. [Program] All done — video, thumbnail, title and description, ready to post.
- ↺ 10→4: the video does not deliver what the title promises? Goes back and adjusts title and thumbnail.
- After delivery: changed only the title's spelling? Redoes only the description. Asked "cut less of the coffee part"?
  Redoes only that stretch, its captions and the final video.

### Marketing campaign (Work and business)
Example: "bring more customers to my father's shop"
Why better: never invents a price or a testimonial. And nothing is published without your permission.
Joins the lines: Text and script, New image, Animated video.
1. [You] Your request — the business and what you want.
2. [AI] Understands the request (default).
3. [Program+AI] Gathers the information — price, opening hours and what it sells. Only what is true.
4. [AI] Plan — whom to talk to, where to promote and how to know it worked.
5. [AI] What to say — the right message for each audience.
6. [AI] Texts and page → Text and script — the texts of the posts and the page.
7. [AI] Images → New image — the images for posts and ads.
8. [AI] Short video → Animated video — a short video showing the offer.
9. [Program+AI] Checks — links working, sizes right, nothing invented.
10. [You] Ready to post — everything ready as drafts. You decide when to publish.
- ↺ 9→5: weak message or wrong information? Goes back and redoes it.
- After delivery: the shop hours changed? Redoes only the texts that show the hours.

### Product launch (Work and business)
Example: "launch the shop's new thermal bottle"
Why better: changing the product's size redoes only the 3D and the photos. Changing the price redoes only the texts.
Joins the lines: Research, Brand and logo, 3D model, Animated video, Marketing campaign, Documents and slides.
1. [You] Your request — the product and who it is for.
2. [AI] Understands the request (default).
3. [AI] Research → Research — competitors, audience and price.
4. [AI] Brand → Brand and logo — name, logo and colours.
5. [Blender] Product in 3D → 3D model — the product in 3D, at real size, with photos.
6. [AI] Video → Animated video — a video showing the product.
7. [AI] Page and ads → Marketing campaign — sales page, ads and posts.
8. [AI] Presentation → Documents and slides — slides to show partners.
9. [Program] All done — everything together, ready to launch when you want.
- ↺ 7→4: the page does not match the brand? Goes back and fixes the brand.

### Data to presentation (Work and business)
Example: "a report of the year's sales with slides"
Why better: if a number is corrected, everything that uses it updates. Changing the look does not redo the math.
Joins the lines: Data and spreadsheets, Documents and slides.
1. [You] Your request — the spreadsheet and the question.
2. [AI] Understands the request (default).
3. [Program] Math → Data and spreadsheets — does and redoes the math.
4. [AI] What it found — what the numbers show, without exaggerating.
5. [AI] Report → Documents and slides — the document with charts.
6. [AI] Slides → Documents and slides — the presentation with the most important points.
7. [Program] Delivers — report, slides and spreadsheet, all consistent.
- ↺ 6→4: a slide says something the numbers do not show? Goes back and reviews.

### Client website (Software)
Example: "a website to sell for the neighbourhood bakery"
Why better: the screen is chosen on a draft before the code. And the site is only delivered after passing what the
client will demand: security, speed, Google and the law.
Joins the lines: Research, Brand and logo, Text and script, Software.
1. [You] Your request — you describe the client's business.
2. [AI] Understands the request — at most 3 questions: what the end customer must be able to do, what already exists
   (photos, texts, Instagram) and whether it takes payments.
3. [AI] Research → Research — competitors, audience and reference sites.
4. [AI] Brand → Brand and logo — only if the business does not have a logo and colours yet.
5. [AI] Texts → Text and script — what the site says on the first screen and the business's real texts.
6. [AI] 3 screen drafts — three very different looks, quick and unpolished, seen at phone size. The best goes on by
   itself, and you can swap.
7. [AI] Paths that make money — lists what cannot fail: the form arrives, the purchase closes, the booking works. Each
   one becomes a test before the code.
8. [AI] Builds → Software — part by part, with test first, another AI checking and the launch check.
9. [Program+AI] Two AIs use it as customers — go through the site as a customer in a hurry, a beginner and someone
   distracted on a phone, and point out where they get stuck.
10. [Program] Test site — the site lives at a test address. The TL-ADE never receives the key to the real site.
11. [You] You release — publishing at the real address is always your decision. If you say nothing, it ends with
    everything ready at the test address.
12. [Program] Client package — step by step to put the domain and hosting in the client's name, the code, the manual
    and the report of what was measured.
- ↺ 9→6: the customer got stuck or did not understand the screen? Back to the screen drafts.
- ↺ 9→8: a money path failed? Back to building only that part.
- After delivery: the price changed? Redoes only the pages that show the price. The brand colour changed? Redoes the
  look and keeps the rest.

## What actually makes these lines good

The value is not the order of stations. It is a small set of moves that recur, plus domain checks:

1. Ask only what changes the result; every question has a default and never blocks. Some lines cap it at 3 questions
   and never ask what the request already says.
2. Learn from real outliers before creating (videos, titles, thumbnails, listings, sources).
3. Diverge cheaply before converging expensively: 3 directions, 6–10 ugly sketches, 3 openings, isolated idea makers.
   The user writes their own ideas before seeing the AI's, so they are not anchored.
4. Test where it will live: phone-size feed, page rendered as image, translation read in place, logo small and in one
   colour, site used as different customer types.
5. Programs do the deterministic part (collect, measure, compute, recompute, render, diff protected areas); models do
   judgment.
6. Protected invariants checked by a program: face, logo, text, numbers, links, frozen script parts.
7. Cross-vendor blind judges; the original wins ties; the writer never approves its own work; pairwise comparisons
   swap the order.
8. Back edges go to the owner of the defect only; a change reruns the owner and its dependents. Repeated defects can
   escalate upstream (bad idea → idea station, before more Blender time).
9. Lines compose lines (hubs): the YouTube, launch, marketing, report and client-site lines are mostly calls to others,
   and some level-2 lines call level-1 lines (Ideas → New image).
10. Human gates with defaults: recording, picking, publishing, buying, releasing. The user is never the bottleneck:
    while a user action is pending, everything that does not depend on it is delivered (the YouTube line hands over
    idea, script and shot list before the recording arrives; Learning keeps the material ready).
11. Honesty limits: says when it could not listen; never invents prices, testimonials or facts; says what it discarded
    and why.
12. Taste memory: opinions on deliveries steer the next one.
13. Domain-expert checks that a generalist would not think of: six-view height alignment and silhouette match for 3D;
    "who, against what, where" readable at thumbnail size; the defect list at the end of a listing; cross-user data
    access on a public site; money paths as tests; the title-vs-finished-video check; audio loudness measured before
    mixing; a new library verified to exist before install.
14. Risk-conditional checks: answers to "who uses it / does it hold other people's data / where does it run" switch
    whole check groups on or off, and are saved in the project.
15. Fact-level reruns after delivery: a changed fact (price, hours, title spelling, brand colour) reruns only the
    pieces that show it. Every "after delivery" example in the map is a fact or an attribute with an owner.
16. Fast path: a small tweak skips the interview and the heavy stations.
17. Secrets stay out: the TL-ADE never holds the real site's key; the person publishes and buys.

## Where fixed lines fail

- 26 lines cover only foreseen requests; anything else (a Terraria mod, a podcast edit with chapters, a wedding
  invitation, a spreadsheet macro) has no line, or gets a wrong one.
- Each line is hand-tuned; improving one does not improve the others, and the same move is written 26 times.
- Lines run stations a request may not need, and miss stations a specific request does need.
- The domain checks in item 13 exist only where someone wrote them.
- Tools are fixed per station (Blender, Photoshop, Premiere), not chosen by what the user has or what works best.
