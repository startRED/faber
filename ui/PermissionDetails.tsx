import { createMemo, For, Match, Show, Switch } from 'solid-js';
import type { Permission } from './api.ts';
import { t } from './i18n.ts';
import { Markdown, openLinks } from './markdown.tsx';
import { changeOf } from './worklog.ts';

const object = (value: unknown): Record<string, unknown> => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
const text = (value: unknown): string => typeof value === 'string' ? value : '';
const raw = (value: unknown): string => typeof value === 'string' ? value : JSON.stringify(value, null, 2) ?? '';
export const isPlan = (tool: string) => tool === 'ExitPlanMode';
export const nativeApproval = (tool: string) => ['command', 'fileChange'].includes(tool) || tool.startsWith('item/');
export const planText = (input: unknown) => text(object(input).plan).trim();

/** Read the original request at the card boundary; unknown fields stay available in a disclosure. */
export function permissionDescription(permission: Permission) {
  const { tool, input } = permission;
  const i = object(input);
  const command = ['Bash', 'PowerShell', 'command'].includes(tool);
  const file = ['Write', 'Edit', 'MultiEdit', 'NotebookEdit', 'fileChange'].includes(tool);
  const mcp = tool.startsWith('mcp__');
  return {
    title: t(isPlan(tool) ? 'ask.plan' : command ? 'ask.command' : file ? 'ask.file' : tool === 'WebFetch' ? 'ask.page' : mcp ? 'ask.mcp' : 'ask.other', { tool }),
    note: text(i.reason) || text(i.description),
    body: command ? (Array.isArray(i.command) ? i.command.map(raw).join(' ') : text(i.command) || text(input)) : file ? text(i.file_path) || text(i.path) || text(i.notebook_path) : text(i.url),
    cwd: text(i.cwd), command, file, mcp,
  };
}

/** Plans use the same sanitized Markdown as answers; edits show their proposed lines before permission is given. */
export function PermissionDetails(props: { permission: Permission }) {
  const d = () => permissionDescription(props.permission);
  const change = createMemo(() => changeOf({ id: props.permission.requestId, name: props.permission.tool, input: props.permission.tool === 'fileChange' ? object(props.permission.input).changes ?? props.permission.input : props.permission.input }));
  const parts = () => props.permission.tool.split('__');
  return (
    <>
      <b>{d().title}</b>
      <Show when={d().note}><p class="note permission-reason">{d().note}</p></Show>
      <Show when={d().cwd}><p class="note permission-path">{t('ask.cwd')}: {d().cwd}</p></Show>
      <Switch>
        <Match when={isPlan(props.permission.tool)}>
          <Show when={planText(props.permission.input)} fallback={<p class="note">{t('ask.plan.missing')}</p>}>
            <div class="md permission-plan" tabindex="0" aria-label={t('ask.plan')} onClick={openLinks}><Markdown text={planText(props.permission.input)} /></div>
          </Show>
        </Match>
        <Match when={d().mcp}>
          <dl class="permission-tool">
            <dt>{t('ask.server')}</dt><dd>{parts()[1]}</dd>
            <dt>{t('ask.tool')}</dt><dd>{parts().slice(2).join('__')}</dd>
          </dl>
          <details><summary>{t('ask.arguments')}</summary><pre tabindex="0">{raw(props.permission.input)}</pre></details>
        </Match>
        <Match when={d().body}><pre class="permission-body" tabindex="0">{d().body}</pre></Match>
      </Switch>
      <Show when={d().file}>
        <Show when={change()} fallback={<p class="note">{t('ask.noPreview')}</p>}>
          {(changed) => <details class="permission-changes" open>
            <summary>{t('ask.changes')}</summary>
            <pre class="permission-diff" tabindex="0"><For each={changed().lines}>{line => <span classList={{ add: line.added, del: !line.added }}>{line.added ? '+' : '−'} {line.text}{'\n'}</span>}</For></pre>
          </details>}
        </Show>
      </Show>
      <Show when={!d().mcp}>
        <details class="permission-raw"><summary>{t('ask.details')}</summary><pre tabindex="0">{raw(props.permission.input)}</pre></details>
      </Show>
    </>
  );
}
