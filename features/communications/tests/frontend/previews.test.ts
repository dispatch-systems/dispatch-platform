import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import type { Message } from '../../api/index.js';
import { preview } from '../../frontend/shared/format.js';

// A message as a row of the list shows it, held to the same cases as a push's preview in Rust
// (`tests/previews.json`), so the list and a push word it alike.

type Case = { text: string; files: [string, string][]; preview: string };
const cases = JSON.parse(
  fs.readFileSync('features/communications/tests/previews.json', 'utf8'),
) as Case[];

test('a message is previewed by its text, else by what it holds, as a push previews it', () => {
  for (const each of cases) {
    const files = each.files.map(([mime, name]) => ({ mime, name })) as Message['files'];
    assert.equal(preview({ text: each.text, files }), each.preview, JSON.stringify(each));
  }
});
