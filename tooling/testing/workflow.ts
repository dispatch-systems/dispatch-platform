/** Read a block-mapping field without coupling checks to the workflow's indentation. */
export function workflowField(source: string, ...keys: string[]): { value: string; body: string } {
  let field = { value: '', body: source };
  for (const key of keys) {
    const lines = field.body.split('\n');
    const significant = lines
      .map((line, index) => ({ line, index }))
      .filter(({ line }) => line.trim() && !line.trimStart().startsWith('#'));
    const indent = Math.min(
      ...significant.map(({ line }) => line.length - line.trimStart().length),
    );
    const entries = significant.filter(
      ({ line }) => line.length - line.trimStart().length === indent,
    );
    const at = entries.findIndex(({ line }) =>
      new RegExp(`^\\s*(?:${key}|"${key}"|'${key}'):\\s*(.*)$`).test(line),
    );
    if (at < 0) throw new Error(`Workflow field ${keys.join('.')} is missing`);
    const entry = entries[at]!;
    field = {
      value: entry.line.slice(entry.line.indexOf(':') + 1).trim(),
      body: lines.slice(entry.index + 1, entries[at + 1]?.index).join('\n'),
    };
  }
  return field;
}

/** The browser matrix is a sequence of integer shard numbers, in flow or block form. */
export function workflowNumbers(field: { value: string; body: string }): number[] {
  const source = [field.value, field.body].join('\n').replace(/#.*$/gm, '').trim();
  const values: unknown = source.startsWith('[')
    ? JSON.parse(source)
    : source.split('\n').map((line) => {
        const item = /^\s*-\s+(\d+)\s*$/.exec(line);
        if (!item) throw new Error('Workflow shard matrix must be an integer sequence');
        return Number(item[1]);
      });
  if (!Array.isArray(values) || !values.every((value) => Number.isInteger(value) && value > 0))
    throw new Error('Workflow shard matrix must be an integer sequence');
  return values as number[];
}
