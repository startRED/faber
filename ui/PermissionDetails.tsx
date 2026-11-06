import { createMemo, For, Match, Show, Switch } from 'solid-js';
import type { Permission } from './api.ts';
import { t } from './i18n.ts';
import { changeOf } from './worklog.ts';

const object = (value: unknown): Record<string, unknown> => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
const text = (value: unknown): string => typeof value === 'string' ? value : '';
const raw = (value: unknown): string => typeof value === 'string' ? value : JSON.stringify(value, null, 2) ?? '';
/** Control, format and line-separator characters (direction marks, zero-width, soft hyphen) show as <U+XXXX>; tabs and line breaks stay. */
const visible = (value: string): string => value.replace(/[\p{Cc}\p{Cf}\p{Default_Ignorable_Code_Point}\u2028\u2029]/gu, c => c === '\t' || c === '\n' || c === '\r' ? c : `<U+${c.codePointAt(0)!.toString(16).toUpperCase().padStart(4, '0')}>`);
const argument = (value: unknown): string => {
  const v = raw(value);
  return !v || /[\s"'\\]/.test(v) ? JSON.stringify(v) : v;
};
/** The files a native file change names (its changes are keyed by path, or listed with one), since the lines alone carry none. */
const paths = (changes: unknown): string => (Array.isArray(changes) ? changes.map((c) => text(object(c).path)) : Object.keys(object(changes))).filter(Boolean).join('\n');
/** The host a network approval is for, shown whether or not a command comes with it. */
const network = (value: unknown): string => {
  const c = object(value);
  return text(c.host) ? page(`${text(c.protocol) ? `${text(c.protocol)}://` : 'https://'}${text(c.host)}`) : raw(value);
};
/** A page address with its ASCII host added when it holds look-alike letters, so it cannot pass for another site. */
const page = (value: string): string => {
  try {
    const host = new URL(value).hostname;
    return /[^\x20-\x7e]/.test(value) ? `${value}\n(${host})` : value;
  } catch { return value; }
};

/** Read the original request at the card boundary; unknown fields stay available in a disclosure. */
export function permissionDescription(permission: Permission) {
  const { tool, input } = permission;
  const i = object(input);
  const command = ['Bash', 'PowerShell', 'command'].includes(tool);
  const file = ['Write', 'Edit', 'MultiEdit', 'NotebookEdit', 'fileChange'].includes(tool);
  const mcp = tool.startsWith('mcp__');
  return {
    title: t(command ? 'ask.command' : file ? 'ask.file' : tool === 'WebFetch' ? 'ask.page' : mcp ? 'ask.mcp' : 'ask.other', { tool }),
    note: text(i.reason) || text(i.description),
    body: command ? [Array.isArray(i.command) ? i.command.map(argument).join(' ') : text(i.command) || text(input), i.networkApprovalContext ? network(i.networkApprovalContext) : ''].filter(Boolean).join('\n') : file ? text(i.file_path) || text(i.path) || text(i.notebook_path) || paths(i.changes) : page(text(i.url)),
    cwd: text(i.cwd), command, file, mcp,
  };
}

/** Edits show their proposed lines before permission is given. */
export function PermissionDetails(props: { permission: Permission }) {
  const d = () => permissionDescription(props.permission);
  const change = createMemo(() => changeOf({ id: props.permission.requestId, name: props.permission.tool, input: props.permission.tool === 'fileChange' ? object(props.permission.input).changes ?? props.permission.input : props.permission.input }));
  const parts = () => props.permission.tool.split('__');
  return (
    <>
      <b>{visible(d().title)}</b>
      <Show when={d().note}><p class="note permission-reason">{visible(d().note)}</p></Show>
      <Show when={d().cwd}><p class="note permission-path">{t('ask.cwd')}: {visible(d().cwd)}</p></Show>
      <Switch>
        <Match when={d().mcp}>
          <dl class="permission-tool">
            <dt>{t('ask.server')}</dt><dd>{visible(parts()[1] ?? '')}</dd>
            <dt>{t('ask.tool')}</dt><dd>{visible(parts().slice(2).join('__'))}</dd>
          </dl>
          <pre class="permission-body" tabindex="0">{visible(raw(props.permission.input).slice(0, 1000))}{raw(props.permission.input).length > 1000 ? `\n${t('ask.more')}` : ''}</pre>
          <details><summary>{t('ask.arguments')}</summary><pre tabindex="0">{visible(raw(props.permission.input))}</pre></details>
        </Match>
        <Match when={d().body}><pre class="permission-body" tabindex="0">{visible(d().body)}</pre></Match>
        <Match when={!d().command && !d().file && props.permission.tool !== 'WebFetch' && raw(props.permission.input)}>
          <pre class="permission-body" tabindex="0">{visible(raw(props.permission.input).slice(0, 1000))}{raw(props.permission.input).length > 1000 ? `\n${t('ask.more')}` : ''}</pre>
        </Match>
      </Switch>
      <Show when={d().file}>
        <Show when={change()} fallback={<p class="note">{t('ask.noPreview')}</p>}>
          {(changed) => <details class="permission-changes" open>
            <summary>{t('ask.changes')}</summary>
            <pre class="permission-diff" tabindex="0"><For each={changed().lines}>{line => <span classList={{ add: line.added, del: !line.added }}>{line.added ? '+' : '−'} {visible(line.text)}{'\n'}</span>}</For></pre>
          </details>}
        </Show>
      </Show>
      <Show when={!d().mcp}>
        <details class="permission-raw"><summary>{t('ask.details')}</summary><pre tabindex="0">{visible(raw(props.permission.input))}</pre></details>
      </Show>
    </>
  );
}
