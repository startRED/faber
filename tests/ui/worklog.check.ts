import { t } from '../../ui/i18n.ts';
import { toolRow } from '../../ui/worklog.ts';
import { check, deepEqual, equal } from './check.ts';

check('worklog: every Claude Code tool reads as what it does, in words', () => {
  equal(toolRow({ id: '1', name: 'Read', input: { file_path: 'C:\\p\\a.txt' } }).label, t('tool.read'));
  equal(toolRow({ id: '2', name: 'Bash', input: { command: 'npm test', description: 'Run the tests' } }).detail, 'npm test', 'a command shows itself');
  deepEqual(toolRow({ id: '3', name: 'WebFetch', input: { url: 'https://akitaonrails.com/en/' } }), { label: t('tool.read'), detail: 'akitaonrails.com/en/' });
  deepEqual(toolRow({ id: '4', name: 'mcp__github__get_issue', input: {} }), { label: t('tool.mcp'), detail: 'github' });
  // A helper started by a helper shows as a step inside the first one's row (R161), named by the words it was given.
  deepEqual(toolRow({ id: '5', name: 'Agent', input: { description: 'reader-a', prompt: 'Read a.txt' }, parent: 'x' }), { label: t('tool.agent'), detail: 'reader-a' });
  equal(toolRow({ id: '6', name: 'SomeNewTool', input: {} }).label, 'SomeNewTool', 'a tool Faber does not know keeps its name');
});
