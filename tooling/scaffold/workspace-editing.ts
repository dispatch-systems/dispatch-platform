import path from 'node:path';
import { marker } from './wire.js';

/** The index of the bracket that closes the one at `start` in Rust, skipping string literals. */
function closing(text: string, start: number) {
  const pairs: Record<string, string> = { '[': ']', '{': '}', '(': ')' };
  const stack: string[] = [];
  for (let index = start; index < text.length; index++) {
    const char = text[index]!;
    if (char === '"') {
      for (index++; index < text.length && text[index] !== char; index++)
        if (text[index] === '\\') index++;
    } else if (pairs[char]) stack.push(pairs[char]!);
    else if (char === stack.at(-1)) {
      stack.pop();
      if (!stack.length) return index;
    }
  }
  throw new Error('unbalanced brackets');
}
/** The items of a one-line list's insides, split at its top-level commas. */
function listItems(inside: string) {
  const out: string[] = [];
  let depth = 0;
  let item = '';
  for (const char of inside) {
    if ('[{('.includes(char)) depth++;
    if (']})'.includes(char)) depth--;
    if (char === ',' && depth === 0) {
      out.push(item.trim());
      item = '';
    } else item += char;
  }
  return [...out, item.trim()].filter(Boolean);
}
/**
 * Adds `entry` as the last item of the list that `opener` opens, indented as the items before
 * it are. `opener` must end at the list's opening bracket. A list rustfmt keeps on one line,
 * being short, is written out one item a line, as rustfmt writes it once it is longer.
 */
export function appendToList(text: string, opener: RegExp, entry: string, file: string) {
  const found = opener.exec(text);
  if (!found) throw new Error(`${file}: cannot find ${opener}; add ${entry} by hand`);
  const start = found.index + found[0].length - 1;
  const end = closing(text, start);
  const lineStart = text.lastIndexOf('\n', end - 1) + 1;
  const inside = text.slice(start + 1, end);
  if (lineStart <= start && !/["/]/.test(inside)) {
    const indent = text.slice(text.lastIndexOf('\n', start) + 1).match(/^\s*/)![0];
    const listed = listItems(inside);
    if (listed.includes(entry.trim().replace(/,$/, '')))
      throw new Error(`${file} already lists ${entry.trim()}`);
    const lines = [...listed.map((item) => `${item},`), entry].map(
      (line) => `${indent}    ${line}\n`,
    );
    return `${text.slice(0, start + 1)}\n${lines.join('')}${indent}${text.slice(end)}`;
  }
  if (lineStart <= start || text.slice(lineStart, end).trim())
    throw new Error(
      `${file}: the list ${opener} opens is not one item a line; add ${entry} by hand`,
    );
  const items = text.slice(start + 1, lineStart).split('\n');
  const indent =
    items
      .reverse()
      .find((line) => line.trim())
      ?.match(/^\s*/)?.[0] ??
    `${text.slice(text.lastIndexOf('\n', found.index) + 1).match(/^\s*/)![0]}    `;
  if (text.slice(start, end).includes(entry.trim()))
    throw new Error(`${file} already lists ${entry.trim()}`);
  return `${text.slice(0, lineStart)}${indent}${entry}\n${text.slice(lineStart)}`;
}
/**
 * Adds a path dependency to a crate's `[dependencies]`, or the `section` named, after its other
 * `dispatch-` ones and above the features' list the wiring writes, with the `features` named
 * enabled.
 */
export function addDependency(
  text: string,
  crate: string,
  location: string,
  file: string,
  { section = 'dependencies', features = [] as string[], optional = false } = {},
) {
  const lines = text.split('\n');
  const header = lines.findIndex((line) => line.trim() === `[${section}]`);
  if (header < 0) throw new Error(`${file} has no [${section}]`);
  let end = lines.findIndex((line, index) => index > header && line.startsWith('['));
  if (end < 0) end = lines.length;
  const listed = lines.slice(header + 1, end);
  if (listed.some((line) => line.startsWith(`${crate} `)))
    throw new Error(`${file} already lists ${crate} in [${section}]`);
  const wired = listed.indexOf(marker);
  const last = (wired < 0 ? listed : listed.slice(0, wired)).findLastIndex((line) =>
    line.startsWith('dispatch-'),
  );
  const enabled = features.length
    ? `, features = [${features.map((feature) => `"${feature}"`).join(', ')}]`
    : '';
  const kept = optional ? ', optional = true' : '';
  lines.splice(header + 1 + last + 1, 0, `${crate} = { path = "${location}"${enabled}${kept} }`);
  return lines.join('\n');
}
/** Has the app's `feature` enable the one of that name on `crate` as well. */
export function forwardFeature(text: string, feature: string, crate: string, file: string) {
  const line = new RegExp(`^${feature} = \\[([^\\]]*)\\]$`, 'm').exec(text);
  if (!line) throw new Error(`${file} has no ${feature} feature; add ${crate}/${feature} by hand`);
  const enabled = [...line[1]!.matchAll(/"([^"]+)"/g)].map((match) => match[1]!);
  if (enabled.includes(`${crate}/${feature}`))
    throw new Error(`${file} already enables ${crate}/${feature}`);
  const list = [...enabled, `${crate}/${feature}`].sort().map((item) => `"${item}"`);
  return text.replace(line[0], `${feature} = [${list.join(', ')}]`);
}
/** Adds `dir` to the workspace's members unless one of them, or a glob, covers it. */
export function addWorkspaceMember(text: string, dir: string) {
  const list = /members\s*=\s*\[([^\]]*)\]/.exec(text);
  if (!list) throw new Error(`Cargo.toml has no workspace members; add "${dir}" by hand`);
  const members = [...list[1]!.matchAll(/"([^"]+)"/g)].map((match) => match[1]!);
  const covered = members.some(
    (member) =>
      member === dir || (member.endsWith('/*') && path.posix.dirname(dir) === member.slice(0, -2)),
  );
  if (covered) return text;
  const end = list.index + list[0].length - 1;
  const separator = list[1]!.trim() ? ', ' : '';
  return `${text.slice(0, end).trimEnd()}${separator}"${dir}"${text.slice(end)}`;
}
