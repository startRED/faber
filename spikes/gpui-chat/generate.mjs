import { writeFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
const out = fileURLToPath(new URL('./out/', import.meta.url));
mkdirSync(out, { recursive: true });
const at = 1790000000000;
const paragraph = 'Rayleigh scattering sends **short blue wavelengths** in every direction, so the sky looks blue. ';
const messages = Array.from({ length: 1000 }, (_, i) => {
  const id = `5f1c${i.toString(16).padStart(4, '0')}-0000-4000-8000-0000000000${(i % 256).toString(16).padStart(2, '0')}`;
  if (i % 2 === 0) return { id, role: 'user', text: `Question ${i / 2}: why is the sky blue?`, at: at + i };
  return { id, role: 'assistant', text: [paragraph.repeat(6), '- one\n- two', `\`\`\`js\nconsole.log(${i});\n\`\`\``].join('\n\n'), at: at + i, vendor: 'claude', model: 'claude-haiku-5-5', status: 'done' };
});
const base = { id: '5f1c0000-0000-4000-8000-000000000001', title: 'Long one', area: 'chat', sessions: {}, createdAt: at, updatedAt: at };
for (const [name, count] of [['thousand.json', 1000], ['small.json', 2]]) {
  const conversation = { ...base, messages: messages.slice(0, count) };
  if (conversation.messages.length !== count) throw new Error('Invalid fixture size');
  writeFileSync(`${out}/${name}`, JSON.stringify(conversation));
}
