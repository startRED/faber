import { createInterface } from 'node:readline';
import { realSpawn, type Spawner } from '../proc.ts';
import { codexArgs } from './launch.ts';

export interface ModelInfo {
  id: string;
  label: string;
  description?: string;
  isDefault?: boolean;
}

// Asks a short-lived `codex app-server` which models this account can use, so the picker shows the real list.
// Resolves to an empty list on any failure: the picker then offers just the account's default.
export function listCodexModels(opts: { bin: string; cwd: string; platform?: NodeJS.Platform; spawn?: Spawner; timeoutMs?: number }): Promise<ModelInfo[]> {
  const proc = (opts.spawn ?? realSpawn)(opts.bin, codexArgs('chat', opts.platform ?? process.platform), opts.cwd);
  const send = (msg: object) => proc.stdin.write(JSON.stringify(msg) + '\n');
  return new Promise((resolve) => {
    const finish = (models: ModelInfo[]) => {
      clearTimeout(timer);
      proc.kill();
      resolve(models);
    };
    const timer = setTimeout(() => finish([]), opts.timeoutMs ?? 20_000);
    proc.once('exit', () => finish([]));
    createInterface({ input: proc.stdout }).on('line', (line) => {
      let msg: { id?: number; result?: { data?: { id: string; displayName?: string; description?: string; hidden?: boolean; isDefault?: boolean }[] } };
      try {
        msg = JSON.parse(line);
      } catch {
        return;
      }
      if (msg.id === 1) {
        send({ method: 'initialized' });
        send({ id: 2, method: 'model/list', params: {} });
      } else if (msg.id === 2) {
        const data = msg.result?.data ?? [];
        finish(data.filter((m) => !m.hidden).map((m) => ({ id: m.id, label: m.displayName ?? m.id, description: m.description, isDefault: m.isDefault })));
      }
    });
    send({ id: 1, method: 'initialize', params: { clientInfo: { name: 'faber', title: 'Faber', version: '0.0.0' } } });
  });
}
