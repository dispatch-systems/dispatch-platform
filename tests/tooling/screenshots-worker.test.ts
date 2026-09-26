import test from 'node:test';
import assert from 'node:assert/strict';
import worker, { type Env } from '../../services/cloudflare-screenshots/worker.js';

const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3]);
const key = `${'a'.repeat(64)}.png`;
const env = {
  SCREENSHOTS: {
    get: async (name: string) =>
      name === key
        ? { httpEtag: '"synthetic"', arrayBuffer: async () => png.slice().buffer }
        : null,
  },
} as unknown as Env;
const fetch = (path: string, method = 'GET') =>
  worker.fetch(new Request(`https://shots.example${path}`, { method }), env);

test('serves a stored screenshot as a PNG cached for good', async () => {
  const response = await fetch(`/${key}`);
  assert.equal(response.status, 200);
  assert.equal(response.headers.get('content-type'), 'image/png');
  assert.equal(response.headers.get('cache-control'), 'public, max-age=31536000, immutable');
  assert.equal(response.headers.get('etag'), '"synthetic"');
  assert.equal(response.headers.get('x-content-type-options'), 'nosniff');
  assert.deepEqual(new Uint8Array(await response.arrayBuffer()), png);
  const head = await fetch(`/${key}`, 'HEAD');
  assert.equal(head.status, 200);
  assert.equal((await head.arrayBuffer()).byteLength, 0);
});

test('answers anything but a GET or HEAD of a stored SHA-256 key with not found', async () => {
  for (const path of [
    '/',
    '/index.html',
    `/${'b'.repeat(64)}.png`,
    `/${key.toUpperCase()}`,
    `/${key}/extra`,
    `/${'a'.repeat(63)}.png`,
    `/${key.replace('.png', '.svg')}`,
  ])
    assert.equal((await fetch(path)).status, 404, path);
  for (const method of ['PUT', 'POST', 'DELETE'])
    assert.equal((await fetch(`/${key}`, method)).status, 404, method);
});
