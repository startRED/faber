# Faber on-screen text audit (en.ts, 2026-10-16)

Files are in `ui/`. Rows ranked by how often a person sees them. "Proposed" is <= 8 words.

## Chat home / Welcome (Welcome.tsx, App.tsx)
| key | current | where | verdict | proposed |
|---|---|---|---|---|
| empty.chatBody | Ask anything. Whether Chat may change files is set in Settings. | Welcome.tsx:66, every new chat | cut 2nd sentence (setting explained on a work screen, C011) | Ask anything. |
| composer.chatPlaceholder | Write here. You can attach spreadsheets, photos and PDFs. | App.tsx:645 ghost hint on home | shorten | Write here, or drop a file |
| s1.try | To start | Welcome.tsx:69 heading above chips | cut | (remove) |
| s1.pickUp | Pick up where you left off | Welcome.tsx:82, Projects.tsx | shorten | Recent |
| suggest.1 | Explain a topic I am stuck on in plain words | chips | shorten | Explain something simply |
| suggest.2 | Help me write a message that is hard to start | chips | shorten | Help me write a message |
| suggest.3 | Compare two options and tell me which fits me | chips | shorten | Compare two options for me |
| hint.chat.2 / .3 | Paste a text and ask what it really says / Describe a problem and ask for three ways out | App.tsx:645 rotating ghost | shorten | Paste a text, ask what it says |
| empty.noAiBody | Connect your Claude or ChatGPT subscription in Settings → Connected accounts to start. | no-AI home | shorten; "ChatGPT" disagrees with "Codex" elsewhere | Connect Claude or Codex to start. |

## Thread (Thread.tsx, Ask.tsx)
| key | current | where | verdict | proposed |
|---|---|---|---|---|
| answer.cost | ${cost} at API prices | Thread.tsx:65, under every answer | hover only | (hover) |
| answer.tokens | {input}k in · {output} out | Thread.tsx:66, every answer | hover only | (hover) |
| answer.cacheWarm | cache warm until {time} | Thread.tsx:148, every answer | cut (jargon, no action) | (remove) |
| reason.pace.* | "{vendor} is being used fast this week; Auto used {other} to save it." | Thread.tsx:205 | shorten | Auto used {other} to save {vendor}. |
| reason.autoDefault / autoOnly | Auto: everyday default / Auto: the only connected subscription | answer header | hover only | (hover) |
| nudge.failedCheck | The last check failed. Runs this message again on {to}. | Thread.tsx:262 | shorten | Check failed. Retry on {to}. |
| nudge.noCheck | Code changed but nothing checked it. Runs this message again on {to}. | tooltip | shorten | Unchecked change. Retry on {to}. |
| answer.nudge | Try again one step up | answer button | shorten | Retry stronger |
| answer.redo | Redo with… | answer button | rename (see disagreements) | Retry with… |
| answer.terminal + terminalHint | Open in terminal / Continue this conversation in the CLI's own terminal | Thread.tsx:288, every answer | move to a "..." menu | (menu) |
| ask.parkedHint | It was not done. Do it yourself, or switch Settings to "Ask before commands" and ask again. | Ask.tsx | shorten; "Ask before commands" is not a mode name | Not done. Switch to Ask and retry. |
| ask.always | Always allow in this conversation | permission card | shorten | Always allow here |
| error.sessionLost | The CLI no longer has this conversation's earlier session. Send your message again: it starts a new session ... | thread error | shorten | Session lost. Send again to continue. |
| note.historyResent | The earlier session was lost; the conversation was sent again as text, without images, without the tools' inputs ... | thread note | shorten, detail behind link | Session restarted from text history. |
| agent.label | Helpers the assistant started | step group | shorten | Helpers |

## Composer (Composer.tsx)
| key | current | where | verdict | proposed |
|---|---|---|---|---|
| effort.nextMessage | Effort applies to the next message. | Composer.tsx:277, permanent in menu | cut (obvious) | (remove) |
| composer.replyPlaceholder | Reply or ask for something else | in-thread composer | shorten | Reply |
| composer.projectPlaceholder | Ask Faber to do something in this folder | project composer | shorten | Ask Faber to work here |
| model.autoChoose | AI: choose for me | model menu | disagrees with model.auto | Auto |
| composer.clear | clear × | attachments | word and glyph duplicate | × |

## Sidebar / Topbar
| key | current | where | verdict | proposed |
|---|---|---|---|---|
| side.usage | Usage shows once the subscription reports it | Sidebar.tsx:303 tooltip | shorten | No usage reading yet |
| quota.countingNote | The number may still rise: the provider counts with a delay, and Faber's recent work may not be in it yet. | Settings.tsx:279 | shorten | Provider counts late; may still rise. |
| side.noProjects, topbar.model, side.hide/show | | | fine | |

## Running column (Running.tsx)
| key | current | where | verdict | proposed |
|---|---|---|---|---|
| run.foot | You can keep talking while the work goes on. | Running.tsx:107, permanent footer | cut | (remove) |
| run.empty | Nothing is running. Start something and it shows up here. | Running.tsx:51 | shorten; duplicates run.none | Nothing running |
| run.working / agent.running | working / working | rows | merge keys | |

## Projects home and board (Projects.tsx)
| key | current | where | verdict | proposed |
|---|---|---|---|---|
| projects.home.lead | Give Faber a goal for this project. It works in a separate copy and shows what it checked. | Projects.tsx:501, permanent | shorten | Give Faber a goal for this project. |
| projects.chat.lead | Ask for anything in this folder. The assistant works right here and asks you when it needs to. | Welcome.tsx:66 | shorten | Ask for anything in this folder. |
| projects.status.* | "2 checks · confirm when you start" / "allowed by your permission mode" / "No checks yet · add them in Settings, Projects" | Projects.tsx:309-316, permanent line | shorten to a state word | Ready / Needs checks |
| projects.sheet.body | A mission proves its work by running them in a clean copy. Faber asks again only if they change. | Projects.tsx:566 | cut (same as projects.proof.desc) | (remove) |
| projects.proof.desc | A mission proves its work by running these commands in a clean copy of the project. Confirm them once; ... | ProjectSettings.tsx | shorten | Mission checks. Confirm once. |
| projects.board.leave | Leave this mission? It stays as it is, but you cannot open it from here again. | Projects.tsx | shorten | Leave this mission? |
| projects.report.untouched | Your own branch was not touched. Look at the branch and merge it when you like. | report | shorten | Your branch is untouched. |
| projects.needsYou.desc, projects.needs.desc | Faber is waiting for your decision before proceeding. / This mission cannot go on without you. | Projects.tsx | cut both; amber "Needs you" says it | (remove) |
| projects.tasks.none | No tasks yet. Faber splits the goal into tasks while it plans. | board | shorten | Planning tasks… |
| projects.home.hint.1 | Describe what to build, fix or improve in this project | ghost | shorten | Describe what to build or fix |

## Permissions strip (App.tsx:68-72, 923)
| key | current | where | verdict | proposed |
|---|---|---|---|---|
| project.permissionsAsk / AcceptEdits / Plan / Auto / DontAsk | "Faber works without asking for approval." etc. | permanent under project composer | move to Settings; keep the mode chip (composer.mode) | (chip only) |
| project.permissionsDetails | Details in Settings | next to the strip | cut with strip | |
| set.permissions.*.line vs project.permissions* | two wordings per mode | Settings vs composer | merge into one set | |

## Settings General
| key | current | where | verdict | proposed |
|---|---|---|---|---|
| set.permissionsDontAskBody | ~90-word paragraph (sandbox, .ssh, best effort...) | Settings, always visible | move behind Learn more | Full access: no prompts, any folder. |
| set.permissionsAskBody | Chat and Projects ask before changing a file ... | Settings | move behind Learn more | Asks before edits and commands. |
| set.chatFolderHint | Choose a folder before the first message. Otherwise, each conversation gets a private folder. Private folders are removed ... | Settings | shorten, rest behind Learn more | Each chat gets a private folder. |
| set.notifyHint | Show a notification when an agent needs you, while Faber is not focused. Sound is a separate choice. | Settings | shorten | When something needs you. |
| set.soundHint | Sound can stay on when notifications are off. Windows uses sound only in full screen; ... | Settings | Learn more | |
| set.keepAwakeHint | Prevents sleep while a chat turn or project is running. The screen may still turn off. | Settings | shorten | Prevents sleep while Faber works. |
| set.previewsHint | Get new builds a few days early. They may be rougher. Turning this off never goes back to an older version. | Settings | shorten | New builds a few days early. |
| set.hooksHint | Off by default. Only used with Ask before commands; with Don't ask ... | Settings | shorten; "Don't ask" is not a mode name | Runs your CLI hooks in Projects. |
| set.nameHint | Shown in the greeting and told to the AI when a conversation starts. Clear it to go without. | Settings | shorten | Used in the greeting and by the AI. |
| set.codexExtras.line | Codex only: how much it says about its reasoning and how long its answers are. | Settings | shorten | Codex only. |
| set.themeHint | Only the look changes. Your chats and projects stay as they are. | Appearance | cut | |
| set.lockedHint + look.locked | They change tone with the background, never meaning. / Amber, coral and violet have a fixed meaning ... | Appearance.tsx | duplicate; keep one | Amber, coral, violet are reserved. |
| look.motion.*Hint | Everything moves: slides, glows and the effect of the new conversation. etc. | Appearance | shorten | |
| set.fileHint | A small file that is easy to send to someone. | Appearance | cut | |

## Settings other sections
| key | current | where | verdict | proposed |
|---|---|---|---|---|
| set.accountsHint | Faber runs the official tools with your own subscriptions. You sign in in their own window; Faber never sees your login. | Settings | duplicates set.privacy.logins | Faber never sees your login. |
| set.usageHint | What your plans report. A value shows once a plan reports it. | Settings | cut | |
| usage.shared | The bars measure the whole account. Other people's use reduces the quota left for Faber. | Settings.tsx:281 | Learn more | |
| usage.expect | {elapsed} of this window has passed · about {share} projected at reset | Settings.tsx:300 | shorten | {share} projected at reset |
| usage.learning / relearning / outside / foreign / meterIncomplete | five separate notes | Settings.tsx:284-286 | collapse behind one Details | |
| usage.overageOn | Paid extra usage is available on this account. Faber asks before using it; turning it off in the provider ... | Settings | shorten | Paid extra usage is available; Faber asks first. |
| usage.authNeeded | Auto needs subscription billing confirmed by the CLI. Naming a plan cannot confirm it. | Settings | shorten | Auto needs confirmed subscription billing. |
| projects.gitMode.equilibrado(+Desc) | Equilibrado (Local commits + draft PR) / Seguro plus pushing ... | Settings Projects | Portuguese in English source | Balanced: local commits + draft PR |
| set.projectsHint | What Faber may do in each project, and the checks that prove its work. | Settings | shorten | Per-project permissions and checks. |
| projects.*Tech (gitMode.seguroTech, selfUpdate.trustTech, proofs.isolationTech) | 40-80 word paragraphs | behind Learn more | fine; trim isolationTech | |

## First run / Tutorial
| key | current | where | verdict | proposed |
|---|---|---|---|---|
| first.foot | You can change everything later in Settings > Appearance. | FirstRun.tsx, Setup.tsx, every step | cut | |
| first.look.sub | Three looks to start from. The screen changes the moment you choose. | FirstRun.tsx | cut | |
| first.motionHint | If your system asks for less motion, this starts in Reduced. | FirstRun.tsx | cut | |
| first.mini.title | The site is almost halfway. | FirstRun preview | filler, meaningless | Preview |
| first.xp.beginnerHint / experiencedHint | Faber shows the way the first few times / No explanations, straight to work | FirstRun | shorten | Show me around / Straight to work |
| setup.body | Choose which official AI tools you want to use with your own subscriptions. | Setup.tsx | shorten | Connect your subscriptions. |
| tour.chat.body | Ask anything in the message box. Faber picks the AI that fits, so you do not have to choose. | Tutorial.tsx | shorten | Ask anything. Faber picks the AI. |
| tour.follow.body vs tour.running.body | near-identical (running list, amber) | Tutorial.tsx | duplicate; keep one | |
| tour.auto.body | Auto picks the model for your first message and tells you why. You can still choose ... | Tutorial | shorten | Auto picks the model and says why. |
| tour.projects.body | Add a folder from your computer and Faber works inside it, like a coding agent. | Tutorial | duplicates empty.openProjectBody | |

## Account / quota cards
| key | current | where | verdict | proposed |
|---|---|---|---|---|
| reason.moved.full / .summary | {from} has no quota. Continued on {to} with the previous messages. / ... a summary of ... | thread | shorten | Moved to {to}: {from} has no quota. |
| quota.uncounted / uncountedShort / counting | three forms of one idea | quota | collapse | |
| quota.credits.confirm | This message will use paid credits ... Send it? | inline, just in time | fine | |

## Duplicated wording
- "Learn more": `set.learnMore` (controls.tsx) and `set.more` (ProjectSettings.tsx). Keep one key.
- "Needs you": run.needsYou, projects.needsYou plus two descriptions.
- "Nothing running": run.none and run.empty.
- Login privacy: set.accountsHint and set.privacy.logins.
- Mission checks explained three times: projects.proof.desc, projects.sheet.body, projects.status.*.
- Running list explained twice in the tour: tour.follow.body, tour.running.body.
- Add-a-folder pitch: empty.openProjectBody, tour.projects.body.
- Permission modes worded twice: `set.permissions.*.line` and `project.permissions*`.

## Labels that disagree
- Retry: "Redo with…" (answer.redo), "Try again one step up" (answer.nudge), "Redo" (menu.redo). Use "Retry".
- Remove vs Delete: "Delete" (conversation), "Remove from Faber" (project), "Remove" (queue), "Remove from recents", "Remove environment". Delete for data, Remove for list entries.
- Auto: "Auto" (model.auto) vs "AI: choose for me" (model.autoChoose).
- Codex vs ChatGPT: setup.chatgpt shows "Codex"; empty.noAiBody says "ChatGPT".
- Full access vs "Don't ask": set.permissions.dontAsk = "Full access"; set.hooksHint says "Don't ask"; ask.parkedHint says "Ask before commands" (the mode is "Ask").
- Settings path styles: "Settings → Connected accounts", "Settings > Appearance", "Settings, Projects". Pick one.
- Mission vs goal: "Mission goal", "Start mission", while the home asks for "a goal".
- Safe vs Seguro vs Equilibrado in the English file.

## Biggest wins
1. Remove cost/tokens/cache line under every answer; hover only.
2. Drop the permanent permission sentence under the project composer; keep the mode chip.
3. One short sentence for empty.chatBody, projects.home.lead, projects.chat.lead.
4. Delete the "To start" heading; shorten chips and placeholders.
5. Delete run.foot, effort.nextMessage, first.foot, first.look.sub, first.motionHint, set.themeHint, set.usageHint, set.fileHint.
6. Merge duplicates (mission checks, Needs-you descriptions, tour.follow/tour.running).
7. Unify labels: Retry, Auto, Codex, Full access, Learn more, one Settings-path style.
8. Fix English source: "Equilibrado", "Seguro", "The site is almost halfway", "Don't ask".
9. Collapse the five usage notes in Settings behind one Details link.
10. Move long Full access / Ask paragraphs and the sound hint behind Learn more.
