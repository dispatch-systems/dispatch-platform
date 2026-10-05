import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// What the app's tests hold the whole product to, such as the kinds of data agents read: lists
// that change with the features it is made of, each kept in a JSON file so that such a change
// shows in review as a change to it. With DISPATCH_UPDATE_SNAPSHOTS set, a test writes what it
// finds there instead: `npm run snapshots:update` sets it, and names each file that changed.
// They are laid out as app/tests/backend/snapshot.rs lays out the backend's.

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');

/** What an assertion against the snapshot at `file` says when they differ. */
export const differs = (file: URL) =>
  `${path.relative(root, fileURLToPath(file))} differs from what the test finds: ` +
  '`npm run snapshots:update` rewrites it, to review with the change';

/** The snapshot at `file`, to hold the product to; while updating, `found` written there first. */
export function snapshot<T>(file: URL, found: T): T {
  if (process.env.DISPATCH_UPDATE_SNAPSHOTS) {
    fs.mkdirSync(path.dirname(fileURLToPath(file)), { recursive: true });
    fs.writeFileSync(file, `${block(found, 0, 0)}\n`);
  }
  if (!fs.existsSync(file)) throw new Error(differs(file));
  return JSON.parse(fs.readFileSync(file, 'utf8')) as T;
}

/**
 * JSON as a reviewer reads it: each value on one line where that fits in 100 columns, otherwise
 * one item a line, as are the outermost and every list of lists or objects, so that adding to a
 * list adds a line. `room` is the columns `value` may take on one line.
 */
function block(value: unknown, indent: number, room: number): string {
  const line = inline(value);
  const items: [string | undefined, unknown][] = Array.isArray(value)
    ? value.map((item) => [undefined, item])
    : value && typeof value === 'object'
      ? Object.entries(value)
      : [];
  const records =
    Array.isArray(value) && value.every((item) => item !== null && typeof item === 'object');
  if (!items.length || (line.length <= room && !records)) return line;
  const inner = ' '.repeat(indent + 2);
  const lines = items.map(([key, item], index) => {
    const label = key === undefined ? '' : `${JSON.stringify(key)}: `;
    // The comma after each but the last takes a column too.
    const used = inner.length + label.length + (index + 1 < items.length ? 1 : 0);
    return `${inner}${label}${block(item, indent + 2, Math.max(0, 100 - used))}`;
  });
  const [open, close] = Array.isArray(value) ? ['[', ']'] : ['{', '}'];
  return `${open}\n${lines.join(',\n')}\n${' '.repeat(indent)}${close}`;
}
/** `value` on one line, with a space after each comma and colon. */
function inline(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(inline).join(', ')}]`;
  if (value && typeof value === 'object')
    return `{${Object.entries(value)
      .map(([key, item]) => `${JSON.stringify(key)}: ${inline(item)}`)
      .join(', ')}}`;
  return JSON.stringify(value);
}
