import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

writeFileSync(join(process.argv[2], 'sales.csv'), 'category,amount\r\nFood,10.00\r\nFood,0.25\r\n"Office, supplies",2.50\r\n"Office, supplies",-0.20\r\n"Client ""A""",1.00\r\nFood,0.00\r\n');
