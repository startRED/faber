// Writes the messy folder of the "organize-folder" request: node organize.mjs <empty folder>.
// Deterministic, including an existing backup whose matching content must survive as a separate file.
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const dest = process.argv[2];
if (!dest) throw new Error('usage: node organize.mjs <folder>');
mkdirSync(dest, { recursive: true });

// A real 1x1 PNG, then padded with a per-file comment chunk so every picture differs.
const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==', 'base64');
const picture = (n) => Buffer.concat([PNG, Buffer.from(`picture ${n}`)]);
const binary = (head, n) => Buffer.concat([Buffer.from(head), Buffer.from(` file number ${n}\n`.repeat(20))]);

const files = {
  'Invoice_ACME_2026-03.pdf': binary('%PDF-1.4 invoice from ACME Ltd, March 2026, total 1,240.00', 1),
  'Invoice_ACME_2026-04.pdf': binary('%PDF-1.4 invoice from ACME Ltd, April 2026, total 980.50', 2),
  'receipt_cafe_0412.pdf': binary('%PDF-1.4 receipt, Cafe Lumen, 12 April, 14.20', 3),
  'flight-confirmation-LIS.pdf': binary('%PDF-1.4 flight confirmation to Lisbon, booking 7QX2LM', 4),
  'tax-return-2025-draft.pdf': binary('%PDF-1.4 tax return 2025 draft, not yet filed', 5),
  'IMG_2041.jpg': Buffer.concat([Buffer.from([0xff, 0xd8, 0xff, 0xe0]), picture(2041)]),
  'IMG_2042.jpg': Buffer.concat([Buffer.from([0xff, 0xd8, 0xff, 0xe0]), picture(2042)]),
  'IMG_2043.jpg': Buffer.concat([Buffer.from([0xff, 0xd8, 0xff, 0xe0]), picture(2043)]),
  'holiday beach.jpg': Buffer.concat([Buffer.from([0xff, 0xd8, 0xff, 0xe0]), picture(7)]),
  'Screenshot 2026-05-02 101500.png': picture(502),
  'Screenshot 2026-05-09 181142.png': picture(509),
  'logo-draft-v2.png': picture(22),
  'meeting notes oct.txt': 'Meeting with the printer on Tuesday.\nAgreed: 500 flyers, delivery by the 14th.\nAsk about the paper weight.\n',
  'meeting notes sept.txt': 'Kick-off for the autumn campaign.\nBudget approved at 3,000.\nNext step: pick the venue.\n',
  'ideas.md': '# Ideas\n\n- a newsletter for the street market\n- a small shop page for the ceramics\n- recipe cards for the stall\n',
  'todo.md': '# This week\n\n- [ ] call the plumber\n- [ ] renew the car insurance\n- [x] send the March invoice\n',
  'shopping list.txt': 'oat milk\nlemons\nbatteries AA\nbirthday candles\n',
  'letter-to-landlord-draft.txt': 'Dear Mr Pereira,\n\nThe heating in the flat has been broken since the 3rd. Could you send someone this week?\n',
  'budget-2026.csv': 'month,income,spending\njan,2100,1800\nfeb,2100,1750\nmar,2300,1900\n',
  'contacts.csv': 'name,phone,email\nAna Costa,555-0101,ana@example.com\nRui Matos,555-0102,rui@example.com\n',
  'export (1).csv': 'date,item,amount\n2026-03-01,paper,12.50\n2026-03-04,ink,40.00\n',
  'market-sales-q1.csv': 'week,stall,sales\n1,A,310\n2,A,295\n3,B,402\n',
  'backup-photos.sh': '#!/bin/sh\n# copies the photos folder to the external drive\nrsync -a ~/Pictures /Volumes/External/photos\n',
  'resize.py': 'from PIL import Image\nimport sys\n\nimg = Image.open(sys.argv[1])\nimg.thumbnail((800, 800))\nimg.save("small_" + sys.argv[1])\n',
  'scrape-prices.js': 'const url = process.argv[2];\nfetch(url).then((r) => r.text()).then((t) => console.log(t.length));\n',
  'old-website-backup.zip': binary('PK\u0003\u0004 old website backup, 2023', 1),
  'laptop-backup-2024-12.zip': binary('PK\u0003\u0004 laptop documents backup, December 2024', 2),
  'installer-notes.log': '2026-02-11 10:02 installed update 4.2\n2026-02-11 10:05 restart required\n',
  'readme-first.txt': 'Things I keep meaning to sort out in here. Start with the invoices.\n',
};

files['meeting notes oct backup.txt'] = files['meeting notes oct.txt'];
for (const [name, content] of Object.entries(files)) writeFileSync(join(dest, name), content);
console.log(`wrote ${Object.keys(files).length} files to ${dest}`);
