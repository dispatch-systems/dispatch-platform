import assert from 'node:assert/strict';
import test from 'node:test';
import type { DocumentsItem } from '../../api/client.js';
import { typeOf } from '../../frontend/items.js';

test("the type column names Google's own files by kind, though Google gives them a size", () => {
  const item = (kind: DocumentsItem['kind'], size: number | null): DocumentsItem => ({
    id: kind,
    name: kind,
    kind,
    size,
    modifiedAt: '2026-10-14T00:00:00Z',
    modifiedBy: null,
    addedBy: null,
    items: null,
    url: `https://drive.google.com/file/d/${kind}/view`,
    location: null,
    thumbnail: null,
    extension: null,
    shortcut: false,
    unavailable: false,
  });
  assert.equal(typeOf(item('sheet', 1024)), 'Google Sheet');
  assert.equal(typeOf(item('doc', null)), 'Google Doc');
  assert.equal(typeOf(item('pdf', 388_000)), '388 KB');
});
