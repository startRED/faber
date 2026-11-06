const fs = require('node:fs');
const path = require('node:path');

const file = path.join(process.cwd(), 'tasks.json');
const [command, ...args] = process.argv.slice(2);

function loadTasks() {
  let contents;
  try {
    contents = fs.readFileSync(file, 'utf8');
  } catch (error) {
    if (error.code === 'ENOENT') return [];
    throw error;
  }
  const tasks = JSON.parse(contents);
  if (!Array.isArray(tasks) || !tasks.every(task =>
    task !== null && typeof task === 'object' &&
    typeof task.text === 'string' && typeof task.done === 'boolean')) {
    throw new Error('tasks.json must contain an array of tasks with text and done fields.');
  }
  return tasks;
}

function saveTasks(tasks) {
  // shortcut: commands run one at a time; add locking if concurrent edits are needed.
  const temporary = `${file}.${process.pid}.tmp`;
  try {
    fs.writeFileSync(temporary, JSON.stringify(tasks, null, 2) + '\n', 'utf8');
    fs.renameSync(temporary, file);
  } finally {
    fs.rmSync(temporary, { force: true });
  }
}

try {
  if (!['add', 'list', 'done', 'remove'].includes(command)) {
    throw new Error('Unknown command. Usage: node todo.js <add <text>|list|done <number>|remove <number>>');
  }
  const text = args.join(' ').trim();
  if (command === 'add' && !text) throw new Error('Provide task text: add <text>');
  if (command === 'list' && args.length) throw new Error('Usage: list');
  if ((command === 'done' || command === 'remove') &&
      (args.length !== 1 || !/^[1-9]\d*$/.test(args[0]) || !Number.isSafeInteger(Number(args[0])))) {
    throw new Error('Provide a valid task number (a positive whole number).');
  }

  const tasks = loadTasks();
  if (command === 'list') {
    tasks.forEach((task, index) => console.log(`${index + 1}. [${task.done ? 'x' : ' '}] ${task.text}`));
  } else if (command === 'add') {
    tasks.push({ text, done: false });
    saveTasks(tasks);
    console.log(`Added task ${tasks.length}.`);
  } else {
    const index = Number(args[0]) - 1;
    if (index >= tasks.length) throw new Error('Task number is out of range.');
    if (command === 'done') tasks[index].done = true;
    else tasks.splice(index, 1);
    saveTasks(tasks);
    console.log(command === 'done' ? `Marked task ${index + 1} done.` : `Removed task ${index + 1}.`);
  }
} catch (error) {
  console.error(`Error: ${error.message}`);
  process.exitCode = 1;
}
