const fs = require('node:fs');
const path = require('node:path');

const file = path.join(process.cwd(), 'tasks.json');
const [command, ...args] = process.argv.slice(2);

function run() {
  if (!['add', 'list', 'done', 'remove'].includes(command)) {
    throw new Error('Unknown command. Use add <text>, list, done <number>, or remove <number>.');
  }
  const text = args.join(' ').trim();
  if (command === 'add' && !text) throw new Error('Please provide task text.');
  if (command === 'add' && /[\r\n]/.test(text)) throw new Error('Task text must be a single line.');
  if (command === 'list' && args.length) throw new Error('Usage: list');
  if (['done', 'remove'].includes(command) &&
      (args.length !== 1 || !/^[1-9]\d*$/.test(args[0]) || !Number.isSafeInteger(Number(args[0])))) {
    throw new Error('Please provide a valid task number (a positive whole number).');
  }

  let tasks;
  try {
    tasks = JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (error) {
    if (error.code === 'ENOENT') tasks = [];
    else throw new Error(`Cannot read tasks.json: ${error.message}`);
  }
  if (!Array.isArray(tasks) || tasks.some(task =>
    !task || typeof task.text !== 'string' || !task.text.trim() ||
    /[\r\n]/.test(task.text) || typeof task.done !== 'boolean')) {
    throw new Error('Invalid tasks.json: expected an array of tasks with text and done fields.');
  }

  if (command === 'list') {
    tasks.forEach((task, index) => console.log(`${index + 1}. [${task.done ? 'x' : ' '}] ${task.text}`));
    return;
  }
  let message;
  if (command === 'add') {
    tasks.push({ text, done: false });
    message = `Added task ${tasks.length}.`;
  } else {
    const index = Number(args[0]) - 1;
    if (index >= tasks.length) throw new Error('No task with that number.');
    if (command === 'done') {
      tasks[index].done = true;
      message = `Marked task ${index + 1} done.`;
    } else {
      tasks.splice(index, 1);
      message = `Removed task ${index + 1}.`;
    }
  }

  // Write beside the original, then replace it so a failed write preserves saved tasks.
  const temporary = `${file}.${process.pid}.tmp`;
  try {
    fs.writeFileSync(temporary, JSON.stringify(tasks, null, 2) + '\n', { flag: 'wx' });
    fs.renameSync(temporary, file);
  } finally {
    if (fs.existsSync(temporary)) fs.unlinkSync(temporary);
  }
  console.log(message);
}

try {
  run();
} catch (error) {
  console.error(`Error: ${error.message}`);
  process.exitCode = 1;
}
