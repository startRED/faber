/** A tiny arithmetic grammar, never executable code. The engine applies the same bounds. */
export function pieceResult(expression: string, x: number): number | undefined {
  if (expression.length > 128 || !Number.isFinite(x) || Math.abs(x) > 1e6) return;
  const tokens = expression.match(/\d+(?:\.\d*)?|\.\d+|[x()+*/-]|\S/g) ?? [];
  if (!tokens.length || tokens.length > 64) return;
  let at = 0;
  const bound = (value: number): number => {
    if (!Number.isFinite(value) || Math.abs(value) > 1e12) throw new Error('bound');
    return value;
  };
  const atom = (depth: number): number => {
    if (depth > 16) throw new Error('depth');
    const token = tokens[at++];
    if (token === '+' || token === '-') return bound((token === '-' ? -1 : 1) * atom(depth + 1));
    if (token === '(') {
      const value = sum(depth + 1);
      if (tokens[at++] !== ')') throw new Error('parenthesis');
      return value;
    }
    if (token === 'x') return x;
    if (!token || !/^(?:\d+(?:\.\d*)?|\.\d+)$/.test(token)) throw new Error('token');
    return bound(Number(token));
  };
  const product = (depth: number): number => {
    let value = atom(depth);
    while (tokens[at] === '*' || tokens[at] === '/') {
      const op = tokens[at++];
      const right = atom(depth);
      value = bound(op === '*' ? value * right : value / right);
    }
    return value;
  };
  const sum = (depth: number): number => {
    let value = product(depth);
    while (tokens[at] === '+' || tokens[at] === '-') {
      const op = tokens[at++];
      const right = product(depth);
      value = bound(op === '+' ? value + right : value - right);
    }
    return value;
  };
  try {
    const value = sum(0);
    return at === tokens.length ? value : undefined;
  } catch { return; }
}
