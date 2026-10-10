import { memo, useEffect, useState } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { api, type Choice, type Message, type ToolUse } from './api.ts';
import { prettyModel, type Option } from './models.ts';

const IMAGE = /\.(png|jpe?g|gif|webp|svg)$/i;

function size(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  return bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

// A file the answer created in the chat workspace: images show inline; anything can be saved to the computer.
function File({ conversationId, file }: { conversationId: string; file: { path: string; size: number } }) {
  const [preview, setPreview] = useState<string>();
  useEffect(() => {
    if (IMAGE.test(file.path)) void api.readImage(conversationId, file.path).then(setPreview);
  }, [conversationId, file.path]);
  return (
    <div className="file">
      {preview && <img src={preview} alt={file.path} />}
      <div>
        <span>{file.path}</span>
        <small>{size(file.size)}</small>
        <button className="btn" onClick={() => void api.saveFile(conversationId, file.path)}>
          Save…
        </button>
        <button className="btn" onClick={() => void api.showFile(conversationId, file.path)}>
          Show
        </button>
      </div>
    </div>
  );
}

function toolLabel(tool: ToolUse): string {
  const input = tool.input as Record<string, unknown> | string | undefined;
  const detail =
    typeof input === 'string' ? input : input && (input.query ?? input.url ?? input.file_path ?? input.command ?? input.pattern);
  return detail ? `${tool.name}: ${String(detail)}` : tool.name;
}

// What a permission request means, in plain words.
function describe(tool: string, input: unknown): { title: string; note?: string; body: string } {
  const i = (input && typeof input === 'object' ? input : {}) as Record<string, unknown>;
  const raw = typeof input === 'string' ? input : JSON.stringify(input, null, 2);
  if (tool === 'Bash' || tool === 'command') {
    return { title: 'Run this command?', note: typeof i.description === 'string' ? i.description : undefined, body: typeof input === 'string' ? input : String(i.command ?? raw) };
  }
  if (['Write', 'Edit', 'MultiEdit', 'NotebookEdit', 'fileChange'].includes(tool)) return { title: 'Change this file?', body: String(i.file_path ?? i.path ?? raw) };
  if (tool === 'WebFetch') return { title: 'Open this page?', body: String(i.url ?? raw) };
  return { title: `Allow ${tool}?`, body: raw };
}

function usage(m: Message): string {
  const parts: string[] = [];
  if (m.costUsd) parts.push(`$${m.costUsd.toFixed(4)} at API prices`);
  if (m.inputTokens) parts.push(`${(m.inputTokens / 1000).toFixed(1)}k in · ${m.outputTokens ?? 0} out`);
  return parts.join(' · ');
}

type OnAnswer = (requestId: string, allow: boolean, always?: boolean) => void;
type OnBranch = (messageId: string, text: string, choice?: Choice) => void;

interface AnswerProps {
  m: Message;
  prompt?: Message;
  conversationId: string;
  options: Option[];
  onAnswer: OnAnswer;
  onBranch: OnBranch;
}

function Ask({ permission, onAnswer }: { permission: NonNullable<Message['permissions']>[number]; onAnswer: OnAnswer }) {
  const { title, note, body } = describe(permission.tool, permission.input);
  return (
    <div className="ask" role="alertdialog" aria-label="Permission request">
      <b>{title}</b>
      {note && <span className="note">{note}</span>}
      <pre>{body}</pre>
      <div>
        <button className="btn primary" onClick={() => onAnswer(permission.requestId, true)}>
          Allow
        </button>
        <button className="btn" onClick={() => onAnswer(permission.requestId, true, true)}>
          Always allow in this conversation
        </button>
        <button className="btn" onClick={() => onAnswer(permission.requestId, false)}>
          Deny
        </button>
      </div>
    </div>
  );
}

const Answer = memo(function Answer({ m, prompt, conversationId, options, onAnswer, onBranch }: AnswerProps) {
  const streaming = m.status === 'streaming';
  return (
    <article className="ai" aria-label="Answer" aria-busy={streaming}>
      <div className="model">
        <b>{prettyModel(m.model)}</b>
        {m.reason && <em>· {m.reason}</em>}
      </div>
      {m.thinking && (
        <details className="thinking">
          <summary>Thinking</summary>
          <div>{m.thinking}</div>
        </details>
      )}
      {!!m.tools?.length && (
        <div className="tools">
          {m.tools.map((t) => (
            <span key={t.id} className={t.ok === false ? 'bad' : ''} title={t.output ?? toolLabel(t)}>
              {toolLabel(t)}
            </span>
          ))}
        </div>
      )}
      {m.permissions?.[0] && <Ask key={m.permissions[0].requestId} permission={m.permissions[0]} onAnswer={onAnswer} />}
      {(m.permissions?.length ?? 0) > 1 && <div className="note">{m.permissions!.length - 1} more waiting</div>}
      <div className="md">
        <Markdown remarkPlugins={[remarkGfm]}>{m.text}</Markdown>
        {streaming && !m.permissions?.length && <span className="cursor" aria-hidden="true" />}
      </div>
      {!!m.files?.length && (
        <div className="files" aria-label="Files">
          {m.files.map((f) => (
            <File key={f.path} conversationId={conversationId} file={f} />
          ))}
        </div>
      )}
      {m.status === 'stopped' && <div className="note">Stopped.</div>}
      {m.error && <div className="note err">{m.error}</div>}
      {!streaming && (
        <div className="actions">
          <button onClick={() => void navigator.clipboard.writeText(m.text)}>Copy</button>
          {prompt && (
            <select
              className="redo"
              aria-label="Redo with another model"
              value=""
              onChange={(e) => onBranch(prompt.id, prompt.text, options[Number(e.target.value)]!.choice)}
            >
              <option value="" disabled>
                Redo with…
              </option>
              {options.map((o, i) => (
                <option key={o.label} value={i}>
                  {o.label}
                </option>
              ))}
            </select>
          )}
          <button onClick={() => void api.openTerminal(conversationId)} title="Continue this conversation in the CLI's own terminal">
            Open in terminal
          </button>
          <span>{usage(m)}</span>
        </div>
      )}
    </article>
  );
});

// The user's message; "Edit" sends the new text as a branch, keeping the original conversation as it was.
function Mine({ m, onBranch }: { m: Message; onBranch: OnBranch }) {
  const [editing, setEditing] = useState<string>();
  if (editing !== undefined) {
    return (
      <form className="you editing" onSubmit={(e) => (e.preventDefault(), editing.trim() && onBranch(m.id, editing))}>
        <textarea aria-label="Edit message" value={editing} rows={Math.min(10, editing.split('\n').length + 1)} onChange={(e) => setEditing(e.target.value)} autoFocus />
        <div className="actions">
          <button type="submit">Send as a branch</button>
          <button type="button" onClick={() => setEditing(undefined)}>
            Cancel
          </button>
        </div>
      </form>
    );
  }
  return (
    <div className="you">
      {m.images?.map((img, i) => <img key={i} alt="Attached image" src={`data:${img.mediaType};base64,${img.base64}`} />)}
      {m.attachments?.map((name) => (
        <span key={name} className="chip attached">
          {name}
        </span>
      ))}
      {m.text}
      <button className="edit" onClick={() => setEditing(m.text)} aria-label="Edit message">
        Edit
      </button>
    </div>
  );
}

export function Thread({ messages, conversationId, options, onAnswer, onBranch }: { messages: Message[] } & Omit<AnswerProps, 'm' | 'prompt'>) {
  return (
    <>
      {messages.map((m, i) =>
        m.role === 'user' ? (
          <Mine key={m.id} m={m} onBranch={onBranch} />
        ) : (
          <Answer key={m.id} m={m} prompt={messages[i - 1]?.role === 'user' ? messages[i - 1] : undefined} conversationId={conversationId} options={options} onAnswer={onAnswer} onBranch={onBranch} />
        ),
      )}
    </>
  );
}
