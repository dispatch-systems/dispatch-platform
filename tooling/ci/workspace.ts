import fs from 'node:fs';
import path from 'node:path';

/** A crate of the Rust workspace: its directory, its package's name and its features. */
export type WorkspaceCrate = { dir: string; name: string; features: string[] };

/** Each [table] of a Cargo.toml with the keys written under it; enough for names and features. */
function tables(text: string) {
  const found = new Map<string, string[]>();
  let table = '';
  for (const line of text.split('\n')) {
    const header = /^\s*\[\[?\s*([^\]]+?)\s*\]\]?\s*(#.*)?$/.exec(line);
    if (header) table = header[1]!;
    const key = /^\s*([A-Za-z0-9_.-]+)\s*=/.exec(line);
    if (!header && key) found.set(table, [...(found.get(table) ?? []), key[1]!]);
  }
  return found;
}

/**
 * The workspace's crates, as the root Cargo.toml lists its members: a member such as
 * `features/*`, with a wildcard at its end or within it as each feature's `contract` has, is every
 * directory it matches with a Cargo.toml.
 */
export function workspaceCrates(root: string): WorkspaceCrate[] {
  const read = (file: string) => fs.readFileSync(path.join(root, file), 'utf8');
  const list = /^\s*members\s*=\s*\[([^\]]*)\]/m.exec(read('Cargo.toml'))?.[1] ?? '';
  const matching = (member: string) =>
    member.split('/').reduce<string[]>(
      (dirs, part) =>
        dirs.flatMap((dir) =>
          part !== '*'
            ? [dir ? `${dir}/${part}` : part]
            : fs.existsSync(path.join(root, dir))
              ? fs
                  .readdirSync(path.join(root, dir))
                  .sort()
                  .map((name) => `${dir}/${name}`)
              : [],
        ),
      [''],
    );
  const members = [...list.matchAll(/"([^"]+)"/g)].flatMap(([, member]) => matching(member!));
  return members
    .filter((dir) => fs.existsSync(path.join(root, dir, 'Cargo.toml')))
    .map((dir) => {
      const text = read(`${dir}/Cargo.toml`);
      const name = /^\[package\][^[]*?^name\s*=\s*"([^"]+)"/m.exec(text)?.[1];
      if (!name) throw new Error(`${dir}/Cargo.toml names no package`);
      return { dir, name, features: tables(text).get('features') ?? [] };
    });
}
