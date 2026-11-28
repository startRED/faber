# T3 Code user features (catalog of docs/user/)

Collected by a Haiku reader from file names and grep hits, not from re-reading every page. Paths are relative to
`apps/` in T3 Code (MIT). `docs/user/` has 41 pages.

| Doc | Feature | Source paths |
|---|---|---|
| activity-log | Activity log | closest `web/src/components/chat/ThreadActivityBanners.tsx` |
| appearance | Themes, fonts, motion, custom and environment themes | `web/src/components/settings/AppearanceSettingsPanel.tsx`, `web/src/appearanceFonts.ts`, `server/src/environmentTheme.ts` |
| background-service | Run in background, service management | `server/src/cloud/bootService.ts`, `server/src/background/BackgroundPolicy.ts`, `web/src/components/settings/BackgroundActivitySettings.tsx` |
| browser-import | Import browser sessions | `web/src/components/settings/BrowserImportWizard.tsx` |
| composer | Attachments, queue, stash, recall, quote, edit | `web/src/components/chat/ChatComposer.tsx`, `ComposerStashMenu.tsx`, `useComposerStash.ts`, `composerPromptHistory.ts` |
| default-browser | Use as default browser | `web/src/browser/browserDefaults.ts` |
| devices | Devices, SSH hosts, device tools | `server/src/device/`, `web/src/components/device/` |
| guest-links | Share a thread read-only with a guest | `server/src/guest/`, `web/src/components/guest/GuestPage.tsx`, `web/src/components/chat/GuestShareControls.tsx` |
| html-renders | Visual replies (HTML and MCP apps) in a sandboxed frame | `server/src/htmlRender/HtmlRender.ts`, `web/src/components/chat/HtmlRenderDialog.tsx`, `McpAppFrame.tsx` |
| keybindings | Custom shortcuts | `server/src/keybindings.ts`, `web/src/components/settings/KeybindingsSettings.tsx` |
| keyboard-focus | Keyboard focus redirect to the composer | `web/src/components/chat/useComposerFocusState.ts`, `composerFocusRedirect.ts` |
| mobile-notifications | Agent notifications on the phone | `mobile/src/features/agent-awareness/`, `server/src/relay/AgentAwarenessRelay.ts` |
| open-source-licenses | Licenses screen | `web/src/components/settings/OpenSourceLicenses.tsx` |
| outside-agents | Connect outside agents through an MCP URL | `web/src/components/auth/ConnectAgentSurface.tsx`, `server/src/cli/thread.ts`, `server/src/mcp/` |
| permission-modes | Permission modes per provider | `server/src/provider/Drivers/ClaudeDriver.ts`, `CodexDriver.ts`, `server/src/orchestration-v2/Adapters/ClaudeAdapterV2.ts` |
| portable-handoffs | Context carried into a handoff | weak match: `web/src/components/chat/MessagesTimeline.logic.ts` |
| project-settings | Project settings, branch naming, worktree location | `web/src/components/settings/ProjectSettingsPanel.tsx`, `BranchNamingSettings.tsx`, `server/src/ProjectSettingsOverrideSweep.ts` |
| prompt-links | Links that open a new thread with a prompt filled in | `web/src/components/promptLinks/` |
| providers-* | Claude, Codex (several ChatGPT accounts), Antigravity, ACP registry, OpenCode, Cursor, Grok | `server/src/provider/` |
| question-attachments | Files inside answers to agent questions | `web/src/components/files/AttachmentFilePreview.tsx` |
| remote-access | Connect tunnel, LAN pairing, remote browser | `server/src/cloud/`, `server/src/cli/pair.ts` |
| snap-shot | Screen capture shortcut into the composer | `web/src/components/desktop/SnapShotCoordinator.tsx` |
| source-control | Git actions, pull requests, review guide | `web/src/components/GitActionsControl.tsx`, `web/src/components/pullRequest/`, `server/src/pullRequest/` |
| telemetry | Usage data and diagnostics export | `server/src/resourceTelemetry/`, `web/src/components/settings/DiagnosticsSettings.tsx` |
| terminal | Terminal drawers kept per thread | `web/src/components/chat/ChatTerminalDrawers.tsx`, `PersistentThreadTerminals.tsx` |
| thread-migration | Threads from older versions | `web/src/components/LegacyThreadMigrationToast.ts` |
| thread-sidebar | Thread list: pin, fold, snooze, archive | `web/src/components/Sidebar.tsx`, `settings/ArchivedThreadsPanel.tsx` |
| updating | App, server and provider CLI updates | `web/src/components/desktopUpdate.logic.ts`, `ProviderUpdatesAction.tsx`, `server/src/desktopUpdate/` |
| usage | Usage page, limits with reset times, price overrides | `web/src/components/usage/`, `server/src/provider/Drivers/claudeUsage.ts`, `codexUsage.ts` |
| welcome-wizard | First-run wizard | `web/src/components/onboarding/WelcomeWizard.tsx`, `FirstRunGate.tsx` |

Also in code without its own user page: scheduled tasks and webhooks (`server/src/scheduledTasks/`), checkpoints and
thread revert (`server/src/checkpointing/`, `server/src/provider/CodexThreadRevert.ts`).
