import { useRef, useState, type ClipboardEvent, type KeyboardEvent } from 'react';
import type { Choice, Vendor } from './api.ts';
import { modelOptions } from './models.ts';

export interface Draft {
  text: string;
  images: { mediaType: string; base64: string }[];
  files: { name: string; base64: string }[];
}

const EMPTY: Draft = { text: '', images: [], files: [] };

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

// Small text files go inside the message (no tool call needed); anything else is attached as a file.
const INLINE_TEXT = 50_000;
const TEXT = /\.(txt|md|csv|json|ya?ml|toml|xml|html?|css|[cm]?[jt]sx?|py|rb|go|rs|java|kt|c|h|cpp|cs|php|sh|ps1|sql|log|ini)$/i;

function readAs(file: File, how: 'text' | 'dataUrl'): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    if (how === 'text') reader.readAsText(file);
    else reader.readAsDataURL(file);
  });
}

// Images go to the model as images, small text files inside the message, and anything else as a file the model opens.
async function attach(list: Iterable<File>, draft: Draft): Promise<{ draft: Draft; names: string[] }> {
  const names: string[] = [];
  let { text, images, files } = draft;
  for (const file of list) {
    if (file.type.startsWith('image/')) {
      const url = await readAs(file, 'dataUrl');
      images = [...images, { mediaType: file.type, base64: url.slice(url.indexOf(',') + 1) }];
    } else if ((file.type.startsWith('text/') || TEXT.test(file.name)) && file.size <= INLINE_TEXT) {
      text = `${text}${text ? '\n\n' : ''}Attached file ${file.name}:\n\`\`\`\n${await readAs(file, 'text')}\n\`\`\``;
    } else {
      const url = await readAs(file, 'dataUrl');
      files = [...files, { name: file.name, base64: url.slice(url.indexOf(',') + 1) }];
    }
    names.push(file.name);
  }
  return { draft: { text, images, files }, names };
}

export function Composer({ vendors, choice, onChoice, busy, placeholder, onSend, onStop, onWarm }: Props) {
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [names, setNames] = useState<string[]>([]);
  const warmed = useRef(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const options = modelOptions(vendors);
  const selected = options.findIndex((o) => o.choice.vendor === choice.vendor && o.choice.model === choice.model);
  const hasContent = draft.text.trim() !== '' || draft.images.length > 0 || draft.files.length > 0;
  const canSend = !busy && hasContent && vendors.length > 0;

  const warm = () => {
    if (!warmed.current) {
      warmed.current = true;
      onWarm();
    }
  };
  const send = () => {
    if (!canSend) return;
    onSend(draft);
    setDraft(EMPTY);
    setNames([]);
    warmed.current = false;
  };
  const add = async (list: Iterable<File>) => {
    const result = await attach(list, draft);
    setDraft(result.draft);
    setNames((n) => [...n, ...result.names]);
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
      {names.length > 0 && (
        <div className="chips">
          {names.map((name, i) => (
            <span key={i} className="chip">
              {name}
            </span>
          ))}
          <span className="chip">
            <button type="button" onClick={() => (setDraft((d) => ({ ...d, images: [], files: [] })), setNames([]))} aria-label="Remove attachments">
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
