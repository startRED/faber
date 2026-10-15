# R4: Terms, usage policy and privacy for CLI wrappers (Anthropic and OpenAI)

Research date: 2026-10-11. Sources read through WebFetch (a small model summarises each page, so
quotes are close to the text but may not be exact) and WebSearch snippets. Items marked
[snippet] came from search results only, not from a full page read. Items marked [403] could
not be fetched.

Caveat: the wording below is a snapshot of pages as served on 2026-10-11. Re-check the
live pages before relying on any single line.

---

## 1. Anthropic

### 1.1 Consumer Terms of Service
- URL: https://www.anthropic.com/legal/consumer-terms (page: "Effective October 8, 2025")
- Sharing (Sec. 2): "You may not share your Account login information"; "You also may not make your Account available to anyone else."
- Automated access (Sec. 3, item 7): bars accessing the Services "through automated or non-human means". Exception: when using an Anthropic API Key, or where Anthropic explicitly permits it.
- Scraping (Sec. 3, item 4): bars crawling/scraping/harvesting, except as the Terms allow.
- Reselling (Sec. 3, item 2): bars using Services to build a competing product or "resell the Services".
- Reverse engineering (Sec. 3, item 3): bars decompiling, except where law forbids the restriction.
- Third-party content (Sec. 7): "Our Services may use or be used in connection with third-party content"; third-party use at own risk.
- No explicit clause on third-party clients or harnesses. No sublicensing clause. Assignment needs consent (Sec. 12).
- Training (Sec. 4): "We may use Materials to provide, maintain, and improve the Services." Training can be turned off "unless you opt out of training through your account settings". Exceptions even when opted out: feedback given, or content flagged for safety review.
- Feedback (Sec. 5): rated conversations stored as part of Feedback.
- Retention on termination (Sec. 12): Anthropic "may at our option delete" data. No fixed period in these Terms.

### 1.2 Commercial Terms of Service
- URL: https://www.anthropic.com/legal/commercial-terms (page: "Effective June 17, 2025")
- No explicit automation or third-party-client clause found in the fetched text.
- Sec. A.1: permits use "to power products and services Customer makes available to its own customers".
- Sec. D.4: no "resell the Services except as expressly approved by Anthropic".
- Sec. D.5: "Customer is responsible for all activity under its account."
- Sec. D.2 incorporates the Usage Policy and Service Specific Terms by reference. Those were not in the fetched text.

### 1.3 Claude Code legal and compliance page (most relevant)
- URL: https://code.claude.com/docs/en/legal-and-compliance (seen 2026-10-11, no date shown)
- Governing terms: Commercial Terms for Team/Enterprise/API; Consumer Terms for Free/Pro/Max.
- Usage policy: "Advertised usage limits for Pro and Max plans assume ordinary, individual usage of Claude Code and the Agent SDK."
- OAuth: "intended exclusively for purchasers of Claude Free, Pro, Max, Team, and Enterprise subscription plans and is designed to support ordinary use of Claude Code and other native Anthropic applications."
- Developers building products, including Agent SDK ones, should use API keys. "Anthropic does not permit third-party developers to offer Claude.ai login into their own applications, or to route requests through Free, Pro, or Max plan credentials on behalf of their users."
- "developers may not collect, store, or intermediate Claude.ai credentials or session tokens — sign-in to a Claude account must complete through Anthropic's own flow."
- Products that preinstall or run Claude Code (section "Can customers offer Claude Code in their products?"), unless otherwise agreed: must agree to Commercial Terms, and:
  - "The Claude Code binary must not be modified." Auth methods may not be removed or restricted.
  - "Customers may not pay for, resell, or intermediate Claude usage on their end users' behalf." Each end user must authenticate with their own API key, subscription credentials, or a 3P provider credential.
- Clarifying text: it does not prevent "an end user from signing in to the unmodified Claude Code binary with their own Claude subscription", including where a platform hosts Claude Code.
- Enforcement: Anthropic "reserves the right to take measures to enforce these restrictions and may do so without prior notice."
- Branding: may say in plain text the product "runs Claude Code"; may not use Claude Code or Anthropic names/logos in your product name.

### 1.4 Agent SDK overview
- URL: https://code.claude.com/docs/en/agent-sdk/overview (seen 2026-10-11)
- Note: "Unless previously approved, Anthropic does not allow third party developers to offer claude.ai login or rate limits for their products, including agents built on the Claude Agent SDK. Use the API key authentication methods described in the Quickstart instead."
- Non-Python/TypeScript languages: official docs say to "run the CLI as a subprocess with the -p flag and --output-format json" (headless docs link).
- Branding: "Claude Code" and "Claude Code Agent" not permitted in partner product names.
- Licence: Agent SDK governed by Commercial Terms "including when you use it to power products and services that you make available to your own customers".

### 1.5 Support article: Agent SDK with your Claude plan
- URL: https://support.claude.com/en/articles/15036540 (title "Use the Claude Agent SDK with your Claude plan"; read via WebFetch summary)
- Update dated June 15, 2026: Anthropic paused previously announced Agent SDK usage changes. "Nothing has changed for now."
- Update dated October 7, 2026: Max and Team plans include monthly API credits. Those credits cover the Agent SDK, `claude -p`, the Claude API and Managed Agents. Summary: the Agent SDK and `claude -p` can still be used with subscription limits.
- Summary reading: Agent SDK, `claude -p` and third-party app use still draw from the subscription's usage limits.
- Caveat: summarised by a model. Verify the exact wording before quoting.

### 1.6 Support article: Use Claude Code with your Pro or Max plan
- URL: https://support.claude.com/en/articles/11145838-using-claude-code-with-your-pro-or-max-plan (seen 2026-10-11, "Updated this week", no date)
- "These limits are shared across Claude and Claude Code." (5-hour and weekly limits)
- If an ANTHROPIC_API_KEY env var is set, Claude Code "will use this API key for authentication instead of your Claude subscription", which bills via API.
- "Avoid adding any Claude Console credentials during the login process" to stay on plan billing.
- "Monthly API credits included with Max plans can't be used in Claude Code."
- Silent on automation, scripts, parallel sessions and account sharing.

### 1.7 Usage Policy (AUP)
- URL: https://www.anthropic.com/legal/aup (page: "Effective November 12, 2026", as read on 2026-10-11, which is after today; verify)
- Relevant lines: "create multiple accounts, to avoid detection, circumvent product guardrails"; "Resell, proxy, or otherwise provide access to Claude through unauthorized means"; "misrepresent the product or client being used" (under the resale rule); "Use outputs to train an AI model" without authorisation.
- Bans on automated account creation and spammy behaviour. No general automation rule found. No clause on usage limits or rate-limit bypass in the fetched text.
- Account-sharing rule: none specific found in the AUP. Consumer Terms cover it.

### 1.8 Data usage page
- URL: https://code.claude.com/docs/en/data-usage (seen 2026-10-11)
- Consumer (Free, Pro, Max): "We give you the choice to allow your data to be used to improve future Claude models." Training includes Claude Code from these accounts "when this setting is on".
- Retention, consumer: 5 years if data use for improvement is allowed; 30 days if not. Settings at claude.ai/settings/data-privacy-controls.
- Commercial (Team, Enterprise, API, 3P): Anthropic "does not train generative models using code or prompts sent to Claude Code under commercial terms", unless opted in (e.g. Development Partner Program).
- Retention, commercial: standard 30 days. ZDR available for qualified Claude for Enterprise accounts.
- /feedback, /bug, /share transcripts: retained 5 years. Default scope = current session; optional 24h or 7 days of other same-project sessions.
- Session-quality survey: transcript upload only on explicit "Yes". Shared transcripts retained up to 6 months.
- Local: session transcripts stored in plaintext under `~/.claude/projects/` for 30 days by default. Adjust with `cleanupPeriodDays`.
- Data flow: prompts and outputs sent over TLS 1.2+.

### 1.9 Privacy Policy (consumer)
- URL: https://www.anthropic.com/legal/privacy (page: "Effective September 10, 2026")
- Deleted conversations: "automatically deleted from our back-end within 30 days." Deleted chats leave history immediately.
- Sec. 2 (training): on by default "unless you opt out through your account settings". Exception: flagged for safety review or reported, even when opted out.
- No Claude Code-specific retention figure on this page.

### 1.10 Telemetry (Claude Code)
- URL: https://code.claude.com/docs/en/data-usage (section "Telemetry services")
- Metrics (latency, reliability, usage patterns) go to Anthropic and to third-party logging infrastructure over TLS. Never include code, prompts or file paths. Off with `DISABLE_TELEMETRY=1`.
- Error reports (internal errors and stack traces) go to a third-party error tracker. Redacted before send. Off with `DISABLE_ERROR_REPORTING=1`. Enabled only when: Pro/Max sign-in, Claude Code v2.1.198+, direct Anthropic API, and no ZDR/HIPAA.
- `/feedback` sends conversation history incl. code to Anthropic (Google Cloud Storage). Off with `DISABLE_FEEDBACK_COMMAND=1`.
- `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`: disables all non-essential traffic including surveys. Also disables feature-flag evaluation, which can make Remote Control unavailable. `DISABLE_ERROR_REPORTING` does not do that.
- `CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY=1`. Survey also off when `DISABLE_TELEMETRY`, `DO_NOT_TRACK` or `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` is set.
- WebFetch domain check: the hostname (not the path) is sent to api.anthropic.com. Always on; not covered by `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`. Off via `skipWebFetchPreflight: true` in settings.
- Env vars can be set in `settings.json`.
- Via a third-party provider (Bedrock, Vertex/Agent Platform, Foundry): telemetry, error reports and `/feedback` default off.
- Claude Code README (https://github.com/anthropics/claude-code/blob/main/README.md, seen 2026-10-11): "clear policies against using feedback for model training"; "limited retention periods for sensitive information". No telemetry env vars listed in README.

---

## 2. OpenAI

### 2.1 Codex sign-in options and automation guidance
- URL: https://learn.chatgpt.com/docs/auth (formerly developers.openai.com/codex/auth, which now redirects; seen 2026-10-11, no date)
- Two options: "Sign in with ChatGPT for subscription access" and "Sign in with an API key for usage-based access".
- Subscription sign-in uses ChatGPT plan credits. API key uses standard API pricing.
- "Use API key authentication for programmatic Codex CLI workflows, such as CI/CD jobs."
- "Access tokens are intended for trusted scripts, schedulers, and private CI runners."
- "API keys are still the recommended default for automation."
- "For general OpenAI API calls, continue to use Platform API keys."
- Storage: plaintext `~/.codex/auth.json` (under CODEX_HOME) or OS credential store. "treat ~/.codex/auth.json like a password." Do not commit or share.
- Codex cloud requires ChatGPT sign-in [snippet].
- `codex login status` shows the active auth method. `codex login --with-access-token` [snippet].
- Enterprise: sign-in with ChatGPT follows workspace permissions, RBAC, and ChatGPT Enterprise retention and residency settings [snippet from auth page].

### 2.2 Non-interactive mode (codex exec)
- URL: https://learn.chatgpt.com/docs/non-interactive-mode (formerly developers.openai.com/codex/noninteractive; seen 2026-10-11, no date)
- "`codex exec` reuses saved CLI authentication by default."
- API keys are the default for CI. ChatGPT-managed auth in CI is labelled "advanced".
- "Use this path only if you specifically need to run as your Codex account."
- Seed `auth.json` via secure storage. Codex must run on the runner to refresh it in place.
- "Do not use this workflow for public or open-source repositories."
- "Do not set OPENAI_API_KEY or CODEX_API_KEY as a job-level environment variable."
- Limits: none stated. Rate limits mentioned as a reason to choose ChatGPT auth, with no values.

### 2.3 Codex CLI page
- URL: https://learn.chatgpt.com/docs/codex/cli (seen 2026-10-11, no date; version shown v0.143.0 in mockup)
- "Use Codex interactively or call codex exec from repeatable workflows and pipelines."
- Plans that include Codex CLI: not stated on this page.
- Usage limits: not discussed. Parallel CLI sessions: not described. Parallel cloud tasks: "Run coding tasks in parallel cloud environments."

### 2.4 Codex SDK and app-server
- URL: https://learn.chatgpt.com/docs/codex-sdk (formerly developers.openai.com/codex/sdk; seen 2026-10-11)
- "Use the Codex SDK to automate coding tasks, including jobs in CI."
- Python SDK "controls the local Codex app-server over JSON-RPC". "Published SDK builds include a pinned Codex CLI runtime dependency." `CodexConfig(codex_bin=...)` overrides the binary.
- App-server: "Use the Codex app server to build custom clients that handle authentication, conversation history, approvals, and streamed agent events."
- Authentication method for the SDK: not stated.
- Navigation label "Sign in with ChatGPT" described as "Apps powered by your user's ChatGPT plan". Not tied to SDK text. Unverified meaning.
- `codex mcp-server` removed (undated).

### 2.5 Help Center: Codex with a ChatGPT plan
- URL: https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan [403 for direct fetch]
- [snippet] "When you sign in to Codex using an existing ChatGPT account, the ChatGPT Terms of Use and Privacy Policy—or the corresponding online services agreement... apply to data shared between Codex and ChatGPT."
- [snippet] Help article covers plan inclusion and usage limits; full text not read.

### 2.6 Terms of Use
- URL: https://openai.com/policies/terms-of-use [403]; https://openai.com/policies/service-terms/ [403]
- [snippet, older versions only] Dec 2024 Terms: users may not share credentials or make an account "available to anyone else"; account holder responsible for all activity.
- [snippet, Dec 2023 terms via third-party summary] automated or programmatic method to extract data or output is barred "unless the method is permitted through the API".
- [snippet] EU terms summary: automated data extraction and misusing output to build competing models are prohibited.
- [snippet] A search result shows a "Previous version" effective January 1, 2026, which implies a newer version. Not read.
- Account Sharing Policy help article [snippet]: an account is for the person who created it; others should sign up for their own. Multiple devices allowed; usage limits may apply.

### 2.7 Privacy and training (Codex)
- Personal plans: the "Improve the model for everyone" setting also covers Codex tasks [snippet, help 7730893 data controls FAQ]. Turn off at Settings → Data Controls or the privacy portal. Either works for ChatGPT and Codex.
- Separate Codex Settings toggle for training on "full environments" [snippet]. Not changed by the ChatGPT setting.
- Business/Enterprise/API: "By default, we do not train on any inputs or outputs" [snippet].
- Retention: Temporary Chats deleted after 30 days [snippet, ChatGPT, not Codex]. Codex-specific retention not found in the official text I read.
- Codex security page [learn.chatgpt.com/docs/security, seen 2026-10-11]:
  - "Telemetry is off by default; enable it explicitly in your configuration."
  - "Codex turns off OTel export by default to keep local runs self-contained."
  - Session transcripts saved locally under CODEX_HOME; can be turned off with `history.persistence`, capped by `history.max_bytes`.
  - "Keep `log_user_prompt = false` unless policy explicitly permits storing prompt contents."
  - "Treat tool arguments and outputs as sensitive."
  - "Route telemetry only to collectors you control."

### 2.8 Config reference (telemetry and data keys)
- URL: https://learn.chatgpt.com/docs/config-file/config-reference (formerly developers.openai.com/codex/config-reference; read 2026-10-11)
- `analytics.enabled`: boolean. Unset means client default applies.
- `feedback.enabled`: boolean, default true. Controls `/feedback` in local clients.
- `history.persistence`: `save-all | none`. Default not stated. Writes `history.jsonl`.
- `history.max_bytes`: cap on history file size; drops oldest entries.
- `log_dir`: default `$CODEX_HOME/log`. Setting it also enables an opt-in plaintext `codex-tui.log`.
- `otel.exporter`: `none | otlp-http | otlp-grpc`. Default not stated.
- `otel.metrics_exporter`: `none | statsig | otlp-http | otlp-grpc`. Default `statsig`.
- `otel.trace_exporter`: `none | otlp-http | otlp-grpc`. Default not stated.
- `otel.log_user_prompt`: boolean, opt-in to export raw prompts.
- `otel.environment`: default `dev`.
- `otel` keys in project-local `.codex/config.toml` are ignored. Set them in user config.
- Note the conflict: "off by default" (security page) versus `metrics_exporter` default `statsig` (config page). Unresolved.

### 2.9 Other OpenAI items
- Config ref: `allow_managed_hooks_only` in `requirements.toml` only (admin).
- Codex and ZDR: GitHub commits in openai/codex disagree on whether Codex CLI supports ZDR orgs [snippet]. Unresolved.
- Codex cloud session retention: GitHub issue openai/codex#24610 asks for documentation of this [snippet]. Not documented as far as read.
- Codex CLI wrappers in the wild reuse the user's existing `~/.codex` login (Promptfoo Codex SDK provider, community Elixir SDK) [snippet]. These are third-party, not OpenAI statements.

---

## Unknown or unclear
- Anthropic: whether a desktop app that only spawns the user's installed, unmodified `claude` binary (`-p`/stream-json), with the user signing in via Claude's flow and the app never touching tokens, counts as "preinstalling or running Claude Code in your products" (needs a Commercial Terms agreement). The text permits end users to sign in with their own subscription but does not say whether an app that launches the CLI for them falls under the clause.
- Anthropic: "ordinary, individual usage" is used but not defined. No text on parallel sessions, concurrency caps or how many headless runs are normal.
- Anthropic: no official text on sharing a plan within a household. The Consumer Terms ban sharing login credentials and making the account available "to anyone else", but do not address family or household use of separate logins.
- Anthropic: the Agent SDK notices (June 15 and Oct 7, 2026) are known only through a summary. Exact wording not verified. Also not verified whether a third-party GUI that launches `claude -p` is "third party developers ... rate limits for their products".
- Anthropic: the AUP is shown as "Effective November 12, 2026", which is a future date relative to 2026-10-11. Verify.
- Anthropic: training default. Consumer Terms say training is on unless opted out. Data-usage page says "We give you the choice". Both read as opt-out; the setting default in the app was not checked.
- Anthropic: no published retention for Claude Code local transcripts beyond the 30-day cleanup default. Whether a third-party harness that reads/writes `~/.claude` is restricted: not addressed.
- OpenAI: Terms of Use and Privacy Policy could not be read (403). The automated-access and account-sharing rules come from older text or summaries. The current 2026 wording is unverified.
- OpenAI: no official statement found on whether a third-party app may let its users sign in with their ChatGPT plan to drive `codex exec` or app-server. The non-interactive page restricts ChatGPT-managed CI auth to "trusted" use and says "Use this path only if you specifically need to run as your Codex account". No explicit ban found either.
- OpenAI: the Codex SDK page does not state its auth method. The app-server page says app-server "handle[s] authentication", which implies sign-in is handled inside app-server, but does not say whether a third party may reuse a user's ChatGPT login.
- OpenAI: Codex telemetry default. "Off by default" vs `otel.metrics_exporter` default `statsig`. Unclear what `statsig` sends and whether `analytics.enabled` unset means on. No single documented switch to turn all OpenAI-bound analytics off was found.
- OpenAI: Codex local history default. `history.persistence` default not stated.
- OpenAI: Codex training on "full environments" toggle: seen only as a search snippet. Applicability to CLI on Plus/Pro not confirmed.
- OpenAI: Codex retention periods (local and cloud) not documented in fetched official text.
- OpenAI: ZDR support for Codex CLI: conflicting repo docs.
- OpenAI: plans that include Codex CLI and usage-limit rules: not read (help center 403).
- No official statement on third-party GUIs wrapping either CLI, and no enforcement case found in official sources for either vendor.

## Questions worth asking
1. Does Anthropic's clause on "preinstalling or running Claude Code in your products" apply to a desktop app that only starts the user's own unmodified `claude` binary in `-p`/stream-json mode, with the user signing in through Claude's own flow and the app never reading or storing tokens? Or does it need a Commercial Terms agreement?
2. Under the 2026-10-07 Agent SDK notice on support.claude.com/en/articles/15036540, does a third-party GUI that launches `claude -p` on the user's own subscription draw from that user's plan limits, and does "Anthropic does not allow third party developers to offer ... rate limits for their products" restrict that?
3. What is the working definition of "ordinary individual usage" for Pro/Max? Is one user running several headless `claude` sessions in parallel on one machine within it, and is there a concurrency figure?
4. Does the Consumer Terms ban on making an account "available to anyone else" cover two adults in one household each signing into their own CLI on the same PC, with each login separate?
5. Which OpenAI Terms of Use version is current after 2026-01-01? Does its automated-extraction bar, "unless the method is permitted through the API", reach `codex exec` or app-server runs that a third-party app starts with the user's own ChatGPT sign-in?
6. Is ChatGPT-managed auth in `codex exec` meant for end-user desktop apps, or only for trusted CI runners ("advanced", "only if you specifically need to run as your Codex account")? Which path does OpenAI support for a desktop wrapper?
7. Which config keys turn off every Codex-to-OpenAI data flow (analytics, statsig metrics, feedback, OTel) for a user on a Plus/Pro plan? Does `otel.metrics_exporter` default `statsig` send data when nothing is set, and is the Codex "full environments" training toggle separate from the ChatGPT setting for CLI use?
8. What is the default for `history.persistence`, and how many days is `history.jsonl` kept? Is there an official Codex retention period for local transcripts and for cloud tasks?
