/** The greeting in two parts, the second one set apart so a look with an italic emphasis (Workshop) can draw it and the
 * stage can set it in a quieter tone. Without a name it is the last word; with one, the name and what follows it, as in
 * "Where do we start, " + "Erick?". */
export function greetingParts(greeting: string, name?: string): [string, string] {
  if (name) {
    const at = greeting.lastIndexOf(name);
    if (at >= 0) return [greeting.slice(0, at).trimEnd(), greeting.slice(at)];
  }
  const at = greeting.lastIndexOf(' ');
  return at < 0 ? [greeting, ''] : [greeting.slice(0, at), greeting.slice(at + 1)];
}
