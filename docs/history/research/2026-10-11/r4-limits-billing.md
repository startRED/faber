# R4: Claude and ChatGPT/Codex plan limits, overage, counting, reset, sharing

Date seen: 2026-10-11 (all sources read this day unless stated). Method: WebFetch on official pages; WebSearch snippets are marked "(search snippet, not fetched)". Pages that returned 403/404 are listed under "Not retrieved".

Source keys:
- [S1] https://claude.com/pricing (fetched; no page date)
- [S2] https://code.claude.com/docs/en/costs (fetched; "Manage costs effectively")
- [S3] https://support.claude.com/en/articles/11145838-using-claude-code-with-your-pro-or-max-plan (fetched; "Updated this week", no date)
- [S4] https://support.claude.com/en/articles/12005970-extra-usage-for-team-and-seat-based-enterprise-plans (fetched; dated 2026-09-04 in article)
- [S5] https://support.claude.com/en/articles/12429409-extra-usage-for-paid-claude-plans (fetched; Japan credit expiry note dated Sept 10 2026)
- [S6] https://code.claude.com/docs/en/errors (fetched; first 100k chars)
- [S7] https://code.claude.com/docs/en/interactive-mode (fetched; section "Wait for a usage limit to reset")
- [S8] https://code.claude.com/docs/en/fast-mode (fetched)
- [S9] https://code.claude.com/docs/en/statusline (fetched; rate_limits fields)
- [S10] https://support.claude.com/en/articles/11049752-how-do-i-sign-up-for-the-max-plan (fetched; dated July 13 2026 in article)
- [S11] https://www.anthropic.com/legal/consumer-terms (fetched; effective Oct 8 2025)
- [S12] https://learn.chatgpt.com/docs/pricing (fetched; this is the redirect target of https://developers.openai.com/codex/pricing; no page date; mentions GPT-5.5 retiring Oct 14 2026)
- [S13] https://learn.chatgpt.com/docs/cloud/pricing (fetched via redirect; see note in section 4)

## 1. Limit shapes per plan

### Claude (Anthropic)

| Plan | Price on page [S1] | Session limit | Weekly limit | Per-model | Numeric counts |
|---|---|---|---|---|---|
| Free | $0 | Rolling 5-hour session window [S1] | None stated [S1] | Sonnet, Haiku only; no Opus, no Fable [S1] | None stated |
| Pro | $20/mo monthly; $17/mo annual, billed $200 up front [S1] | Rolling 5-hour session; "at least 5x more usage per 5-hour session than Free" [S1] | Yes, "Weekly limits also apply" [S1]; S3 confirms both Pro and Max have a 5-hour and a weekly limit | Opus, Sonnet, Haiku; Fable only through usage credits [S1] | None on current official pages |
| Max 5x | From $100/mo [S1] | "5x more usage than Pro" per 5-hour session [S1] | Yes; separate weekly Fable limit [S3] | Opus, Sonnet, Haiku; Fable at "50% of weekly limits" [S1] | None on current official pages |
| Max 20x | Shown under the same "From $100" heading; no separate price on the fetched page [S1] | "20x more usage than Pro" per 5-hour session [S1] | Yes; separate weekly Fable limit [S3] | As Max 5x [S1] | None on current official pages |
| Team standard seat | $20/seat/mo annual; $25 monthly [S1] | Session window (rolling five-hour) [S2] | Weekly window [S2] | Opus, Sonnet, Haiku; Fable at 50% of weekly limits on premium seats [S1] | "more usage than Pro*" (no number) [S1] |
| Team premium seat | $100/seat/mo annual; $125 monthly [S1] | Session window [S2] | Weekly window [S2] | As above [S1] | "5x more usage than standard seats*" [S1] |
| Enterprise (seat-based) | $20/seat/mo annual, plus usage billed at API rates [S1] | Rolling five-hour window plus weekly window, per seat tier [S2] | Yes [S2] | Opus, Sonnet, Haiku, Fable listed; no Fable limit stated [S1] | Admins set user and org spend limits [S1] |
| Enterprise (usage-based) | n/a on fetched page | No included allowance; all usage billed at API rates from the first token [S4] | n/a | n/a | n/a |

Notes on the Claude table:
- The "Max 20x" price is not shown on the fetched S1 page. The page lists "Monthly API credits" of $100/mo (Max 5x) and $200/mo (Max 20x) [S1]. These credits cannot be used in Claude Code or the Claude apps [S3].
- Pooled API credits on Team are "up to $500/month" [S1].
- Team and Enterprise session and weekly windows are shared with Claude chat and Cowork [S2].
- Older Help Center text (search snippet, not fetched): Max 5x "at least 225 messages every 5 hours", Max 20x "at least 900 messages every 5 hours" for short conversations. The newer text drops the numbers and says counts vary with message length, attachments, conversation length, and model or feature [search snippet, support.anthropic.com/en/articles/11014257 family].
- Older Help Center text (search snippet): "more than 50 sessions per month" may limit access, "not a strict cut-off" [search snippet, same family].
- Weekly window length: S9 calls the window `seven_day` (7 days) and gives `resets_at` as Unix epoch seconds. Whether it is rolling or fixed-anchor is not stated.
- Session window: S1 and S2 say "rolling". S9 gives a `five_hour` object with `resets_at`. The search snippet says the 5-hour window starts at the first message (search snippet, Help Center 11014257, not fetched). The Help Center pages returned 404 on the URLs tried.
- Peak-hour adjustment to 5-hour limits (about 7% of users): reported by a Substack post citing an X post. Not verified against an Anthropic page.
- Per-model message in the errors page: "You've hit your Opus limit", "You've hit your Sonnet limit" exist, so family-level limits exist. The page does not state their size or whether they are weekly [S6].

### ChatGPT / Codex (OpenAI)

Source: [S12] (learn.chatgpt.com, the redirect target of developers.openai.com/codex/pricing).

| Plan | Price on page | Limit shape (from S12) |
|---|---|---|
| Free | $0 | No specific limits listed. GPT-6 Luna at Standard speed in desktop app, "subject to rollout" |
| Go | $8/mo | No specific limits listed. Same Luna desktop access as Free |
| Plus | $20/mo | Five-hour windows. Estimated local messages per window: GPT-6 Astra 5-45; GPT-6.1 Sol 15-160; GPT-6 Sol 15-150; GPT-6 Luna 350-3,000. "These estimates are not fixed message limits." "Weekly limits may also apply." Local messages and cloud chats share the allowance |
| Pro | "from $100/month; $100, $200, or $500 tiers" | "Pro plans currently have no five-hour limit." Weekly allowance still applies per the help-article snippet (search snippet, see section 4). Ultrafast mode only on the $500 tier |
| Business | $20/user/mo annual, $25 monthly; 2+ users | Per-window estimates match Plus. Credits can extend usage |
| Enterprise and Edu (flexible pricing) | n/a | "no fixed rate limits. Usage scales with credits" |
| Enterprise and Edu (no flexible pricing) | n/a | "the same per-seat usage limits as Plus for most features" |
| API key (any plan) | API pricing | Billed at API pricing; no limit shape given |

Notes on the OpenAI table:
- The page gives no monthly limit and no fixed message or task caps [S12].
- Reset times are shown only in the usage dashboard [S12]. `/status` in Codex CLI shows the same (third-party and community reports, not official).
- Community post (not official): `/status` in Codex CLI now shows only the weekly limit and credits, with no five-hour bucket.

## 2. Overage, extra usage, credits

Claude:
- Usage credits (the Help Center calls them "extra usage" in older versions) are for individuals on Pro, Max 5x and Max 20x [S5]. Team and seat-based Enterprise seats are eligible [S4]. Usage-based Enterprise is not eligible [S4].
- Billing: "Usage credits are billed at standard API rates", charged separately from the subscription [S5]. You prepay [S5].
- Spend cap: monthly cap, or unlimited [S5]. Auto-reload: "Automatically add funds when your balance drops below a threshold" [S5].
- Team: Owners pre-purchase credits and set spend limits at org, seat-tier, group or user level [S4]. Seat-based Enterprise: credits billed at the end of each month [S4].
- At a cap: members cannot use Claude, Cowork or Claude Code "until the next billing period" unless limits change [S4]. Limits are checked before a request and cost after, so one request can overshoot the cap, then later requests are blocked [S4].
- Pro and Max with credits on: requests "switch to pay-as-you-go credits and keep working". A notification may show briefly, then "Now using credits" [S5].
- Fast mode uses usage credits only, not plan limits, even if plan usage remains [S8].
- Opus: S5 does not say whether Opus draws credits. It says Opus costs more tokens than Sonnet and Haiku [S5].
- Fable: once the Max Fable weekly limit is used up with credits on, requests move to credits [S5]. Consent prompt text: "Fable limit reached · continuing on Fable 5.1 uses usage credits" [S6].
- How a harness can tell overage is in use:
  - Statusline JSON (Pro and Max only, after the first API response): `rate_limits.five_hour.used_percentage`, `rate_limits.seven_day.used_percentage`, and `resets_at` for each. Values go 0 to 100 [S9].
  - Behind a Claude apps gateway: `rate_limits.spend_limit.used_percentage` (can go above 100), `used_usd`, `limit_usd`, `period` (some optional) [S9]. Not relevant to Pro/Max direct.
  - No `overageStatus`, `isUsingOverage` or equivalent field appears in S9 or S2 (checked by grep of S9 output). Unknown whether one exists elsewhere.
  - Error strings that indicate spend or credits: "You've hit your monthly spend limit · raise it at ...", "You've hit your individual spend limit · ask your admin for a higher limit", "You've hit your org's monthly spend limit", "You've hit your team's shared budget", "Credit balance is too low" (listed, truncated) [S6].
  - `/usage` shows a usage-credits row while credits are on [S2]. `/usage-credits` opens settings [S2].
- Team and Enterprise members see "Request usage credits" when at the limit; the request status changes to "Request sent to admin" [S4].

Codex:
- Credits are the pay-as-you-go layer. After included limits run out, Plus and Pro can buy more credits; Business, Edu and flexible Enterprise workspaces can buy workspace credits [S12].
- Credits per 1M tokens (standard rate): GPT-6.1 Sol 50 input, 2.5 cached input, 250 output; GPT-6 Luna 2.5, 0.25, 12.5. "A typical GPT-5.6 Sol task may use 2-15 credits" [S12].
- Voice in Desktop: $0.05/min on Plus and Pro, drawn from the Codex usage budget; 1.25 credits/min on Business, Edu, Enterprise credit plans [S12].
- Purchased credits are non-refundable (search snippet, help article 20001507; not fetched).
- Harness signal: none documented that I could fetch. Community post says `/status` shows weekly limit and credits.

## 3. How usage is counted

Claude:
- Shared pool: "These limits are shared across Claude and Claude Code, meaning all activity in both tools counts against the same limits" [S3]. IDE usage counts too [S3].
- Subagents: "The subagent's own requests still draw on your usage" [S2]. Agent teams: "Agent teams use approximately 7x more tokens than standard sessions when teammates run in plan mode" [S2].
- `/usage` shows attribution to skills, subagents, plugins and MCP servers as percentages, plus behavior flags for long context and cache misses at 10% or more [S2]. Figures are local-machine only; claude.ai and other devices are not included [S2].
- Long context: a 1M-context request needs usage credits turned on, "Usage credits required for 1M context" [S6]. Context beyond 200K auto-compacts to standard context [S6].
- Thinking tokens: billed as output tokens in the API [S2]. Thinking cannot be turned off on Opus 5.5, Sonnet 5.5, Haiku 5.5 or Fable [S2]. Subscription weighting of thinking is not stated.
- Cache: cache reads are "re-read at the cached token rate" in the API pricing sense [S2]. How cached tokens count against plan limits is not stated.
- Input vs output weighting for plan limits: not stated anywhere I read. `used_percentage` in the status line uses input tokens only for context, not plan limits [S9].
- Fast mode: on Opus 5.5 $8/$40 per MTok (input/output); on Opus 5 and Opus 4.8 $10/$50. Fast mode is available on subscription plans only through usage credits and is not in the subscription limits [S8]. Fast mode is a per-model configuration and does not change the model [S8]. A "Fast mode ON" session pays the full uncached input price for the whole context once [S8].
- Parallel sessions: nothing in the official pages sets a per-session weight. The limit is per account [S3]. Scheduled tasks, cross-session messages and goal check-ins each send full context and draw on usage [S2].
- Tool calls and model weights: not stated in S3. S2 says "each time Claude uses tools it sends another request carrying that batch of tool results" [S2], which implies more requests and more context, not a separate tool-call price.

Codex:
- Meter counts tokens and compute, not messages [search snippet on developers.openai.com-related pages; S12 gives credits per 1M tokens for credits, not for included limits].
- Local messages and cloud tasks share the five-hour window [S12 footnote as quoted in search snippets].
- Fast mode: 2.5x against included usage, 2x against purchased credits or Enterprise pay-as-you-go [S12].
- Ultrafast (GPT-6 Astra and GPT-6.1 Sol): 8x against included usage, 6x against purchased credits. Eligible: "Pro $500 and eligible Enterprise/Edu plans" [S12].
- Image generation: about 3-5x faster use of included limits on average [S12].
- Code Review counts toward usage only when run through GitHub. Local or non-GitHub reviews count toward general limits [S12].
- Parallel sessions and subagents: not stated in S12.

## 4. At the limit

Claude (official, S6 unless noted):
- Messages:
  - "You've hit your session limit · resets 3:45pm"
  - "You've hit your weekly limit · resets Mon 12:00am"
  - "You've hit your Opus limit · resets 3:45pm"
  - "You've hit your Sonnet limit · resets 3:45pm"
- Session and weekly limits are shared across models; switching model does not restore access. Opus and Sonnet limits apply to their family only, so switching to a model outside that family works [S6].
- Early warning: "You've used 85% of your session limit · resets 3:45pm" [S6].
- No automatic model fallback. For an Opus or Sonnet limit, run `/model` and pick a model outside that family [S6].
- Resume: by default Claude Code does not resume by itself. In an interactive session signed in with a claude.ai subscription (v2.1.234+), it waits in the open session and continues the interrupted task after the reset [S6, S7]. It does not resend the last message [S7]. It re-arms at most twice in a row, then stops with "Automatic continue stopped after repeated usage-limit hits" [S7].
- The wait does not survive exiting Claude Code [S7]. It is not started for resets more than 24 hours away (for example, weekly) [S7]. It is not offered in background sessions or `-p` runs [S7]. API keys and cloud providers have no wait [S7].
- Setting `autoContinueAtUsageLimit` controls it [S7].
- Status line: `rate_limits.five_hour.resets_at` and `rate_limits.seven_day.resets_at` give the exact reset (Unix epoch seconds). Claude Code drops a window once its `resets_at` passes [S9].
- 429 errors (not plan usage): "API Error: Request rejected (429) · this may be a temporary capacity issue" applies to API key, Bedrock or Google Cloud rate limits, not plans [S6]. "Server is temporarily limiting requests (not your usage limit)" is a short server throttle [S6].
- Team and Enterprise seat-limit state: "You won't be able to use Claude, Cowork, or Claude Code again until the next billing period" once spend limit is exceeded [S4].

Codex:
- Official error text and retry behaviour: not retrieved. The help article on paid resets (20001507) returned 403 to WebFetch, and the search snippet is the only source (not fetched).
- Paid reset (search snippet, help article 20001507): buying a reset restores both 5-hour and weekly usage immediately. It pulls the normal weekly allowance forward, adding no capacity. "The weekly clock restarts with your next request" and the next automatic weekly reset is 7 days after the first request. Eligibility conflicts across article versions (one says not on Plus/Pro? one says eligible Plus and Pro personal accounts). Saved resets from June 12 2026 (third-party only).
- Third-party only: "Model at capacity" errors (developersdigest.tech). Not verified.

Fallback to another model: not found in S12.

## 5. Plan changes mid-cycle

Claude:
- Pro to Max: "Your Pro plan will be upgraded to Max immediately, with billing adjusted on a prorated basis for the remainder of your current billing cycle" [S10, dated July 13 2026]. Annual Pro to Max credit on unused balance [S10].
- Downgrades (Max to Pro, or 20x to 5x): "Not addressed" in S10.
- Effect on session and weekly windows after a change: not addressed in S10, S3, S6, S7 or S9.
- How fast the CLI notices a change: not documented. The status line `rate_limits` data is refreshed with each API response [S9]. `/usage` refreshes on request [S2]. Mid-cycle timing is unknown.
- Unverified GitHub issue: user upgraded 5x to 20x mid-cycle and weekly limits depleted faster (search snippet; a single user's report, not official).

Codex:
- Upgrades: OpenAI help text (search snippet, not fetched): upgrade from the pricing page or Settings > My Plan; "Once you select a Pro plan, the new usage limits and capabilities apply immediately." Switching between $100 and $200 Pro tiers from Settings > My Plan (search snippet).
- Downgrades and proration: no official source found. Community reports (not official): after Pro to Plus downgrade, consumption from the larger ceiling counted against the Plus ceiling; renewing Pro did not reset weekly limit; upgrading left limits at Plus level (community.openai.com, unverified; no staff replies).

## 6. Shared accounts and concurrency

Claude:
- Consumer Terms, effective Oct 8 2025 [S11], Section 2 "Your Anthropic Account": "You may not share your Account login information, Anthropic API key, or Account credentials with anyone else." And: "You also may not make your Account available to anyone else." The account holder is responsible for all activity [S11].
- Section 3: "You are responsible for all activity under the account through which you access the Services" [S11].
- Section 3 item 7 bars automated or non-human access except through an API key or where explicitly permitted [S11].
- Section 2 "Business Domains": an employer or organization admin "may be able to monitor and control the Account" [S11].
- Team and Enterprise seats are per user; the organization is responsible for all activity under its account (search snippet, third-party summary). The consumer terms say they do not cover API and Console organizations, which fall under the Commercial Terms [search snippet on S11].
- Devices and simultaneous sessions: no clause in S11 sets a device or session count [S11]. S3 says: log in to IDEs with the same credentials used in the terminal, and gives no other multi-device rule [S3].
- Limits are per account: "limits are shared" [S3]. Two people on one login would draw on the same window (inference from S3 and S11, not stated officially).
- Third-party (search snippet, unverified): two people using one login share one usage limit, one conversation history, and one memory.
- Anecdotal Reddit reports of bans for sharing (unverified).

Codex / OpenAI:
- OpenAI Terms of Use: not retrieved (403). No sharing or concurrency rule verified.
- Shared pool: "Codex, ChatGPT Work, ChatGPT for Excel, and Workspace Agents draw from the same agentic usage and credit pool" (search snippet citing help center; not fetched).
- No device or concurrency limit found in S12.

## Not retrieved (403/404)
- https://support.claude.com/en/articles/11014257-about-claude-s-pro-plan-usage (404)
- https://support.claude.com/en/articles/11014257-about-claude-s-max-plan-usage (404)
- https://support.claude.com/en/articles/11014257-about-claude-max-plan-usage (404 on claude.com/support mirror; 301 to support.claude.com; 404 after redirect)
- https://support.claude.com/en/articles/8324991-about-claude-pro-plan-usage (404)
- https://help.openai.com/en/articles/20001507-paid-weekly-work-and-codex-rate-limit-resets (403)
- https://help.openai.com/so-dj/articles/20001507-... (403)
- https://openai.com/policies/terms-of-use/ (403)
- https://code.claude.com/docs/en/costs is fetched; /docs/en/errors fetched. https://developers.openai.com/codex/cloud/pricing redirected to learn.chatgpt.com/docs/cloud/pricing; the redirect target returned data (section 2, S13) but the answer was not stored in full here.

## Unknown or unclear
- Current numeric caps per Claude plan. No official number in fetched pages. Older Help Center snippet numbers (225 and 900 messages per 5 hours) are unverified and marked old.
- Claude session window start rule (first message vs fixed anchor). Official pages say "rolling", and the first-message rule is from a search snippet of an unfetched article.
- Claude weekly window: rolling or fixed anchor. The `seven_day` object gives `resets_at`, but S9 does not say how it is computed.
- Whether Opus has its own weekly limit or only the family message exists.
- How plan usage weights input, output, cache reads and writes, thinking, tool calls.
- Max 20x price on S1 (not shown on fetched page).
- Team seat minimum: sources conflict (one says at least two members from September 2026; another says five). Not resolved.
- Codex five-hour window status: July 2026 reports say temporarily removed for Plus, Pro and Business; one August report says Plus restored from Aug 25 2026; S12 currently says Pro has no five-hour limit. Not resolved.
- Codex Pro weekly figures and the $100 tier multiplier (a promotion once made it 10x Plus; search snippets only).
- Codex error messages at limit and their retry-after values.
- Codex and Claude downgrade behaviour mid-cycle and when the CLI notices a change.
- OpenAI sharing and concurrency rules (terms page 403).
- Whether Opus draws usage credits on Pro and Max. S5 silent.
- Whether a Claude statusline or other field marks "using overage" for Pro and Max. None found in S9.
- Whether Claude subagents and parallel sessions count separately from the main session. S2 says subagent requests draw on usage; no per-session quota.

## Questions worth asking
1. Faber's Claude routing signal: is the statusline `rate_limits` object (Pro and Max only, after the first response) acceptable as the sole quota source? Team and API accounts get no such object, so what should routing do for them?
2. Overage spend: should Faber ever use Claude usage credits or Codex credits without asking each time, or always stop and ask? This is a money decision for Erick.
3. Shared login: should Faber support one Claude login used by two people? Consumer Terms forbid sharing login, so the design may need one login per person, or Team seats.
4. Which Codex plan does the household hold (Plus, Pro $100/$200/$500, Business, or Enterprise)? Routing thresholds differ, since Plus has a five-hour window and Pro currently does not.
5. The Codex five-hour window is unsettled. Should routing use only the weekly figure, or read live `/status` at runtime?
6. Faber cannot read the plan tier from the CLI, so should the plan be declared in settings, or inferred from the status line?
7. Fable and Opus: should routing treat the separate Fable weekly limit (50% of weekly on Max) as its own bucket?
8. Mid-cycle plan change: after an upgrade, should Faber drop cached quota numbers and re-read them, since the CLI's reset behaviour is undocumented?
