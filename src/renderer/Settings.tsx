import { useState } from 'react';
import type { Settings as Saved } from './api.ts';
import { ACCENTS, PRESETS, presetOf, type Appearance } from './appearance.ts';

type Option<T> = [T, string];

function Choices<T extends string>({ label, value, options, onPick }: { label: string; value: T | undefined; options: Option<T>[]; onPick: (v: T) => void }) {
  return (
    <div className="row">
      <span>{label}</span>
      <div className="opts" role="group" aria-label={label}>
        {options.map(([v, text]) => (
          <button key={v} className="opt" aria-pressed={v === value} onClick={() => onPick(v)}>
            {text}
          </button>
        ))}
      </div>
    </div>
  );
}

const SECTIONS = ['Appearance', 'Projects'] as const;

export function Settings({ appearance, saved, onAppearance, onSave, onClose }: { appearance: Appearance; saved: Saved; onAppearance: (a: Appearance) => void; onSave: (patch: Saved) => void; onClose: () => void }) {
  const [section, setSection] = useState<(typeof SECTIONS)[number]>('Appearance');
  const set = (patch: Partial<Appearance>) => onAppearance({ ...appearance, ...patch });
  return (
    <div className="settings">
      <nav className="s-nav nav" aria-label="Settings">
        <button className="item" onClick={onClose}>
          <span>← Back</span>
          <kbd>Esc</kbd>
        </button>
        {SECTIONS.map((s) => (
          <button key={s} className="item" aria-current={s === section} onClick={() => setSection(s)}>
            <span>{s}</span>
          </button>
        ))}
      </nav>
      <div className="s-main">
        <h2>{section}</h2>
        {section === 'Appearance' ? (
          <>
            <Choices label="Preset" value={presetOf(appearance)} onPick={(p) => onAppearance(PRESETS[p])} options={[['mono', 'Mono (default)'], ['studio', 'Studio'], ['cockpit', 'Cockpit']]} />
            <Choices label="Mode" value={appearance.mode} onPick={(mode) => set({ mode })} options={[['dark', 'Dark'], ['black', 'Black'], ['light', 'Light'], ['system', 'Follow system']]} />
            <div className="row">
              <span>Accent color</span>
              <div className="opts" role="group" aria-label="Accent color">
                {(Object.keys(ACCENTS) as Appearance['accent'][]).map((a) => (
                  <button
                    key={a}
                    className="sw"
                    style={{ ['--c' as string]: ACCENTS[a][0] }}
                    aria-label={a === 'mono' ? 'No accent (monochrome)' : a[0]!.toUpperCase() + a.slice(1)}
                    aria-pressed={!appearance.custom && appearance.accent === a}
                    onClick={() => set({ accent: a, custom: null })}
                  />
                ))}
                <label className="custom">
                  Any color <input type="color" value={appearance.custom ?? ACCENTS[appearance.accent][0]} onChange={(e) => set({ custom: e.target.value })} />
                </label>
              </div>
            </div>
            <Choices label="Interface font" value={appearance.ui} onPick={(ui) => set({ ui })} options={[['geistmono', 'Geist Mono'], ['jetbrains', 'JetBrains Mono'], ['sans', 'Sans (no mono)']]} />
            <Choices label="Reading font" value={appearance.read} onPick={(read) => set({ read })} options={[['geist', 'Geist'], ['mona', 'Mona Sans'], ['same', 'Same as interface']]} />
            <Choices label="Density" value={appearance.density} onPick={(density) => set({ density })} options={[['comfortable', 'Comfortable'], ['compact', 'Compact']]} />
            <Choices label="Corners" value={appearance.corners} onPick={(corners) => set({ corners })} options={[['round', 'Round'], ['square', 'Square']]} />
            <p className="hint">Saved in settings.json in Faber's data folder, so a theme is a plain file anyone can share.</p>
          </>
        ) : (
          <div className="row">
            <span>Your CLI hooks</span>
            <label className="check">
              <input type="checkbox" checked={saved.userHooks === true} onChange={(e) => onSave({ userHooks: e.target.checked })} />
              <span>
                Run the hooks you set up in Claude Code and Codex during project work.
                <small>Off by default: a hook can slow down or block every turn. Chat never runs them.</small>
              </span>
            </label>
          </div>
        )}
      </div>
    </div>
  );
}
