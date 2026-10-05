/** SQLite's own tables and table-valued functions, which no owner declares. */
const builtIn = /^(sqlite_\w+|pragma_\w+|json_each|json_tree|main|temp)$/;

/**
 * Whether a string literal is SQL, or a piece of it: our SQL writes its keywords in capitals,
 * which English sentences and identifiers do not.
 */
export function isSql(text: string): boolean {
  return (
    /^\s*(SELECT|INSERT|UPDATE|DELETE|REPLACE|CREATE|ALTER|DROP|WITH|PRAGMA)\b/.test(text) ||
    /\b(FROM|JOIN|INTO)\s+[`"]?[a-z_][a-z0-9_]*/.test(text) ||
    /\bUPDATE\s+[a-z_][a-z0-9_]*\s+SET\b/.test(text)
  );
}

/** The tables and views a piece of SQL creates, by name. */
export function createdTables(sql: string): string[] {
  const text = strip(sql);
  return [
    ...[
      ...text.matchAll(
        /\bCREATE\s+(?:TEMP\s+|TEMPORARY\s+|VIRTUAL\s+)*(?:TABLE|VIEW)\s+(?:IF\s+NOT\s+EXISTS\s+)?((?:\w+\.)?\w+)/gi,
      ),
    ].map((match) => match[1]!),
    ...[...text.matchAll(/\bALTER\s+TABLE\s+(?:\w+\.)?\w+\s+RENAME\s+TO\s+(\w+)/gi)].map(
      (match) => match[1]!,
    ),
  ]
    .map(unqualified)
    .map((name) => name.toLowerCase());
}

/**
 * The tables a piece of SQL names: what it reads, writes, creates, alters, drops, indexes or
 * references. Names it defines for itself with `WITH` are not tables.
 */
export function namedTables(sql: string): string[] {
  const text = strip(sql);
  const local = new Set(
    [...text.matchAll(/(?:\bWITH(?:\s+RECURSIVE)?|,)\s*(\w+)\s*(?:\([^)]*\))?\s+AS\s*\(/g)].map(
      (match) => match[1]!,
    ),
  );
  const names: string[] = [];
  const name = /^\s*([`"]?)((?:[a-z_][a-z0-9_]*\.)?[a-z_][a-z0-9_]*)\1/;
  const take = (at: number, list: boolean) => {
    for (;;) {
      const match = name.exec(text.slice(at));
      if (!match) return;
      const after = text.slice(at + match[0].length);
      // A table-valued function, such as json_each(…), is no table.
      if (list && /^\s*\(/.test(after)) return;
      names.push(unqualified(match[2]!));
      if (!list) return;
      // `FROM a x, b y`: every table of the list, past its alias.
      const rest = /^\s*(?:AS\s+)?(?:[a-z_][a-z0-9_]*\s*)?,/.exec(after);
      if (!rest) return;
      at += match[0].length + rest[0].length;
    }
  };
  for (const match of text.matchAll(/\b(FROM|JOIN)\s+/g)) take(match.index + match[0].length, true);
  for (const match of text.matchAll(
    /\b(?:INTO|UPDATE(?:\s+OR\s+[A-Z]+)?|(?:TABLE|VIEW)(?:\s+IF\s+(?:NOT\s+)?EXISTS)?|REFERENCES)\s+/g,
  ))
    take(match.index + match[0].length, false);
  for (const match of text.matchAll(/\bINDEX\b[^;]*?\bON\s+/g))
    take(match.index + match[0].length, false);
  return names.filter((table) => !local.has(table) && !builtIn.test(table));
}

// SQL with its string values, comments and placeholders blanked, so that only names remain.
function strip(sql: string): string {
  return sql
    .replace(/--[^\n]*/g, ' ')
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/'(?:[^']|'')*'/g, "''")
    .replace(/\{[^}]*\}/g, ' {} ');
}
const unqualified = (table: string) => table.split('.').at(-1)!;
