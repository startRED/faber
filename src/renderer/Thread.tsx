import { memo, useEffect, useState } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { api, type Message, type ToolUse } from './api.ts';
import { prettyModel } from './models.ts';

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

interface AnswerProps {
  m: Message;
  conversationId: string;
  onAnswer: OnAnswer;
}

function Ask({ permission, onAnswer }: { permission: NonNullable<Message['permission']>; onAnswer: OnAnswer }) {
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

const Answer = memo(function Answer({ m, conversationId, onAnswer }: AnswerProps) {
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
      {m.permission && <Ask permission={m.permission} onAnswer={onAnswer} />}
      <div className="md">
        <Markdown remarkPlugins={[remarkGfm]}>{m.text}</Markdown>
        {streaming && !m.permission && <span className="cursor" aria-hidden="true" />}
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
          <span>{usage(m)}</span>
        </div>
      )}
    </article>
  );
});

export function Thread({ messages, conversationId, onAnswer }: { messages: Message[] } & Omit<AnswerProps, 'm'>) {
  return (
    <>
      {messages.map((m) =>
        m.role === 'user' ? (
          <div key={m.id} className="you">
            {m.images?.map((img, i) => <img key={i} alt="Attached image" src={`data:${img.mediaType};base64,${img.base64}`} />)}
            {m.text}
          </div>
        ) : (
          <Answer key={m.id} m={m} conversationId={conversationId} onAnswer={onAnswer} />
        ),
      )}
    </>
  );
}
