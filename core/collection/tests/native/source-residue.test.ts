import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileContainsAny } from '../support/files.js';

test('provider residue scanning finds UTF-8 and UTF-16 markers across chunk boundaries', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-residue-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const file = path.join(root, 'profile');
  const marker = 'Provider Marker Å Driver';
  const needles = [Buffer.from(marker, 'utf8'), Buffer.from(marker, 'utf16le')];
  for (const needle of needles) {
    // The marker starts inside a chunk and is longer than several chunks.
    fs.writeFileSync(file, Buffer.concat([Buffer.alloc(7, 1), needle, Buffer.alloc(13, 2)]));
    assert.equal(fileContainsAny(file, needles, 8), true);
    fs.writeFileSync(file, Buffer.concat([Buffer.alloc(7, 1), needle.subarray(0, -1)]));
    assert.equal(fileContainsAny(file, needles, 8), false);
  }
  fs.writeFileSync(file, Buffer.alloc(100, 3));
  assert.equal(fileContainsAny(file, needles, 8), false);
});
