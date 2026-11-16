import path from 'node:path';
import { realpathSync } from 'node:fs';

// Resolve existing ancestors too: a new folder below a junction still aliases its target.
function canonical(folder: string): string {
  let current=path.resolve(folder);
  const tail: string[]=[];
  for (;;) {
    try {return path.join(realpathSync.native(current),...tail).toLowerCase();}
    catch(error) {if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;const parent=path.dirname(current);if(parent===current)throw new Error('unresolved data path');tail.unshift(path.basename(current));current=parent;}
  }
}
export function isolatedTestData(explicit: string | undefined, normal: string): boolean {
  if(!explicit?.trim() || !path.isAbsolute(explicit))return false;
  try {
    const candidate=canonical(explicit), base=canonical(normal);
    return candidate!==base && !candidate.startsWith(base+path.sep) && !base.startsWith(candidate+path.sep);
  } catch {return false;}
}
