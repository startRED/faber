import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

writeFileSync(join(process.argv[2], 'README.md'), '# Invoice helper\n\nNamed export: total(lines). Amounts are integer cents.\n');
writeFileSync(join(process.argv[2], 'invoice.mjs'), 'export const total = lines => lines.reduce((sum, x) => sum + (x.quantity || 1) * x.unitCents, 0);\n');
