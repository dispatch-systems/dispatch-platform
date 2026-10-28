import fs from 'node:fs';
import path from 'node:path';
import { MARKER, marked } from './doctor.js';

// `dispatchdev todo [<owner>]`: what the generators left for their author to write, each marker
// by its file, its line and what it asks for, under the owner whose folder holds it. `new` and
// `add` list those in what they wrote.

/** A marker: where it is, and what it asks for. */
export type Todo = { file: string; line: number; text: string };
/**
 * An owner's markers: the owner as `dispatchdev test --owner` names it, `feature:parking`, and
 * apart, its kind and its name, which a top folder such as `app` or `mcp` has none of.
 */
export type Owned = { owner: string; kind: string; name: string; todos: Todo[] };

/** What opens a line comment, which the next lines of a marker's comment open too. */
const COMMENT = /^\s*(\/\/[!/]?|--)(?=\s|$)/;
/** What closes the comment or the string a marker sits in, at the end of its line. */
const CLOSER = /\s*(\*\/\}?(<\/>)?|",?)\s*$/;

/**
 * The markers in a file's text, each with what it asks for: the rest of its line, and the next
 * lines of its comment, or of its paragraph in prose, while its sentence goes on.
 */
export function markers(file: string, text: string): Todo[] {
  const lines = text.split('\n');
  return lines.flatMap((line, index) => {
    const at = line.indexOf(MARKER);
    if (at < 0) return [];
    const lead = COMMENT.exec(line)?.[1];
    const prose = !lead && !line.slice(0, at).trim() && file.endsWith('.md');
    const rest = line.slice(at + MARKER.length).replace(/^[\s:]+/, '');
    let asked = rest.replace(CLOSER, '');
    let open = asked === rest.trimEnd();
    for (const next of lines.slice(index + 1, index + 4)) {
      if (!open || /[.?!]$/.test(asked) || next.includes(MARKER)) break;
      const goesOn = lead
        ? COMMENT.exec(next)?.[1] === lead
        : prose && next.trim() !== '' && !/^\s*([#>|*-]|\d+\.)/.test(next);
      if (!goesOn) break;
      const more = (lead ? next.replace(COMMENT, '') : next).trim();
      open = !CLOSER.test(more);
      asked = `${asked} ${more.replace(CLOSER, '')}`;
    }
    return [{ file, line: index + 1, text: asked }];
  });
}

/** The kind of owner each folder of owners holds. */
const KIND_OF: Record<string, string> = {
  features: 'feature',
  collectors: 'collector',
  core: 'core',
};
const KINDS = [...Object.values(KIND_OF), 'integration'];
/**
 * The owner whose folder holds `file`: `feature:parking`, `collector:cortex`, `core:shell`, an
 * integration by its folder's name as `integration:file_intake`, or a top folder, as `mcp`.
 */
function ownerOf(file: string) {
  const parts = file.split('/');
  const [top = '', name] = parts;
  if (top === 'features' && parts[2] === 'integrations' && parts.length > 4)
    return `integration:${parts[3]}`;
  const kind = KIND_OF[top];
  return kind && parts.length > 2 ? `${kind}:${name}` : top;
}

/** Every marker in `root`, by owner, in the order their files sort. */
export function todos(root: string): Owned[] {
  const found = new Map<string, Todo[]>();
  for (const file of marked(root)) {
    const owner = ownerOf(file);
    const text = fs.readFileSync(path.join(root, file), 'utf8');
    found.set(owner, [...(found.get(owner) ?? []), ...markers(file, text)]);
  }
  return [...found].map(([owner, todos]) => {
    const [kind = '', name = ''] = owner.split(':');
    return { owner, kind, name, todos };
  });
}

/** Whether `wanted` names `owner`: as itself, or alone by its name or its kind, as `parking`. */
export function named(wanted: string, owner: string) {
  const [kind, name] = owner.split(':');
  return owner === wanted || (!wanted.includes(':') && (name === wanted || kind === wanted));
}
/** Whether `wanted` names an owner the checkout has, as `named` reads it. */
export function known(root: string, wanted: string) {
  if (KINDS.includes(wanted)) return true;
  const [kind, name = ''] = wanted.includes(':') ? wanted.split(':') : ['', wanted];
  if (!/^[a-z][a-z0-9_]*$/.test(name)) return false;
  const features = path.join(root, 'features');
  const folders: Record<string, string[]> = {
    feature: [`features/${name}`],
    collector: [`collectors/${name}`],
    core: [`core/${name}`],
    integration: (fs.existsSync(features) ? fs.readdirSync(features) : []).map(
      (feature) => `features/${feature}/integrations/${name}`,
    ),
  };
  const candidates = kind ? (folders[kind] ?? []) : [name, ...Object.values(folders).flat()];
  return candidates.some((dir) => fs.existsSync(path.join(root, dir)));
}

/** A marker as a line of what's left: `  features/parking/feature.rs:1: what it does`. */
const line = (todo: Todo) => `  ${todo.file}:${todo.line}: ${todo.text}`;
/** What `new` and `add` say last: what they left to write, if anything. */
export const leftLines = (left: Todo[]) =>
  left.length ? ['Left to write:', ...left.map(line)] : [];
/** The owners' markers as people read them: a heading an owner, then a line a marker. */
export const listed = (owned: Owned[]) =>
  owned
    .flatMap(({ owner, todos }) => [`${owner.replace(':', ' ')}:`, ...todos.map(line)])
    .join('\n');
