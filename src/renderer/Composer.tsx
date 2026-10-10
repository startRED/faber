import { useRef, useState, type ClipboardEvent, type KeyboardEvent } from 'react';
import type { Choice, Vendor } from './api.ts';
import { modelOptions } from './models.ts';

export interface Draft {
  text: string;
  images: { mediaType: string; base64: string }[];
}

interface Props {
  vendors: Vendor[];
  choice: Choice;
  onChoice: (choice: Choice) => void;
  busy: boolean;
  placeholder: string;
  onSend: (draft: Draft) => void;
  onStop: () => void;
  onWarm: () => void;
}

const MAX_TEXT_FILE = 200_000;

function readAs(file: File, how: 'text' | 'dataUrl'): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    if (how === 'text') reader.readAsText(file);
    else reader.readAsDataURL(file);
  });
}

// Images go to the model as images; any other file is read as text and attached to the message.
async function attach(files: Iterable<File>, draft: Draft): Promise<{ draft: Draft; files: string[] }> {
  const names: string[] = [];
  let { text, images } = draft;
  for (const file of files) {
    if (file.type.startsWith('image/')) {
      const url = await readAs(file, 'dataUrl');
      images = [...images, { mediaType: file.type, base64: url.slice(url.indexOf(',') + 1) }];
    } else {
      const body = (await readAs(file, 'text')).slice(0, MAX_TEXT_FILE);
      text = `${text}${text ? '\n\n' : ''}Attached file ${file.name}:\n\`\`\`\n${body}\n\`\`\``;
    }
    names.push(file.name);
  }
  return { draft: { text, images }, files: names };
}

export function Composer({ vendors, choice, onChoice, busy, placeholder, onSend, onStop, onWarm }: Props) {
  const [draft, setDraft] = useState<Draft>({ text: '', images: [] });
  const [files, setFiles] = useState<string[]>([]);
  const warmed = useRef(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const options = modelOptions(vendors);
  const selected = options.findIndex((o) => o.choice.vendor === choice.vendor && o.choice.model === choice.model);
  const canSend = !busy && (draft.text.trim() !== '' || draft.images.length > 0) && vendors.length > 0;

  const warm = () => {
    if (!warmed.current) {
      warmed.current = true;
      onWarm();
    }
  };
  const send = () => {
    if (!canSend) return;
    onSend(draft);
    setDraft({ text: '', images: [] });
    setFiles([]);
    warmed.current = false;
  };
  const add = async (list: Iterable<File>) => {
    const result = await attach(list, draft);
    setDraft(result.draft);
    setFiles((f) => [...f, ...result.files]);
  };
  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      send();
    }
  };
  const onPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const images = [...e.clipboardData.files].filter((f) => f.type.startsWith('image/'));
    if (images.length) {
      e.preventDefault();
      void add(images);
    }
  };

  return (
    <form
      className="composer"
      onSubmit={(e) => {
        e.preventDefault();
        send();
      }}
      onDragOver={(e) => e.preventDefault()}
      onDrop={(e) => {
        e.preventDefault();
        void add(e.dataTransfer.files);
      }}
    >
      {files.length > 0 && (
        <div className="chips">
          {files.map((name, i) => (
            <span key={i} className="chip">
              {name}
            </span>
          ))}
          <span className="chip">
            <button type="button" onClick={() => (setDraft((d) => ({ ...d, images: [] })), setFiles([]))} aria-label="Remove attachments">
              clear ×
            </button>
          </span>
        </div>
      )}
      <textarea
        aria-label="Message"
        rows={Math.min(10, Math.max(2, draft.text.split('\n').length))}
        placeholder={placeholder}
        value={draft.text}
        onChange={(e) => {
          setDraft((d) => ({ ...d, text: e.target.value }));
          warm();
        }}
        onFocus={warm}
        onKeyDown={onKey}
        onPaste={onPaste}
        autoFocus
      />
      <div className="cbar">
        <button type="button" className="pill" onClick={() => fileInput.current?.click()}>
          + Attach
        </button>
        <input ref={fileInput} type="file" multiple hidden onChange={(e) => e.target.files && void add(e.target.files)} />
        <div className="r">
          <select
            className="pill"
            aria-label="Model"
            value={selected < 0 ? 0 : selected}
            onChange={(e) => {
              onChoice(options[Number(e.target.value)]!.choice);
              warmed.current = false; // the next keystroke warms the newly chosen model
            }}
          >
            {options.map((o, i) => (
              <option key={o.label} value={i}>
                {o.label}
              </option>
            ))}
          </select>
          {busy ? (
            <button type="button" className="send" onClick={onStop} aria-label="Stop">
              ■
            </button>
          ) : (
            <button type="submit" className="send" disabled={!canSend} aria-label="Send">
              ↑
            </button>
          )}
        </div>
      </div>
    </form>
  );
}
