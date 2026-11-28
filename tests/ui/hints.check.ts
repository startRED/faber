import { useLanguage } from '../../ui/i18n.ts';
import { homeHints } from '../../ui/idle.ts';
import { check, deepEqual, ok } from './check.ts';

// What the idle composer of a new conversation suggests (docs/product-map.md, "Projects: new conversation").

check('hints: a project’s new conversation suggests project work, never the Chat hints', () => {
  useLanguage('en');
  const project = homeHints('project');
  const chat = homeHints('chat');
  deepEqual(project, ['Ask Faber to work here', 'Explain how this folder is organized', 'Find the failing test and fix it']);
  ok(project.every((hint) => !chat.includes(hint)), 'no hint of the other area');
});
