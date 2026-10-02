import test from 'node:test';
import assert from 'node:assert/strict';
import worker, { type Env } from '../../services/cloudflare-screenshots/worker.js';

const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3]);
const key = `${'a'.repeat(64)}.png`;
const metadata = { httpEtag: '"synthetic"', size: png.byteLength };
let gets = 0;
let heads = 0;
const env = {
  SCREENSHOTS: {
    head: async (name: string) => {
      heads++;
      return name === key ? metadata : null;
    },
    get: async (name: string) => {
      gets++;
      return name === key
        ? {
            ...metadata,
            body: new ReadableStream({
              start(controller) {
                controller.enqueue(png.slice());
                controller.close();
              },
            }),
            arrayBuffer: async () => assert.fail('The worker must stream the object body'),
          }
        : null;
    },
  },
} as unknown as Env;
const fetch = (path: string, method = 'GET', headers?: HeadersInit) =>
  worker.fetch(new Request(`https://shots.example${path}`, { method, headers }), env);

test('serves a stored screenshot as a PNG cached for good', async () => {
  const response = await fetch(`/${key}`);
  assert.equal(response.status, 200);
  assert.equal(response.headers.get('content-type'), 'image/png');
  assert.equal(response.headers.get('cache-control'), 'public, max-age=31536000, immutable');
  assert.equal(response.headers.get('etag'), '"synthetic"');
  assert.equal(response.headers.get('x-content-type-options'), 'nosniff');
  assert.deepEqual(new Uint8Array(await response.arrayBuffer()), png);
  const before = gets;
  const head = await fetch(`/${key}`, 'HEAD');
  assert.equal(head.status, 200);
  assert.equal((await head.arrayBuffer()).byteLength, 0);
  assert.equal(head.headers.get('content-length'), String(png.byteLength));
  assert.equal(gets, before, 'HEAD must not fetch the body');
  assert(heads > 0);
});

test('conditional requests use metadata and return no body for matching ETags', async () => {
  for (const method of ['GET', 'HEAD']) {
    for (const tag of ['"synthetic"', 'W/"synthetic"', '"other", W/"synthetic"', '*']) {
      const before = gets;
      const response = await fetch(`/${key}`, method, { 'If-None-Match': tag });
      assert.equal(response.status, 304, `${method}: ${tag}`);
      assert.equal(response.headers.get('etag'), '"synthetic"');
      assert.equal((await response.arrayBuffer()).byteLength, 0);
      assert.equal(gets, before);
    }
    const changed = await fetch(`/${key}`, method, { 'If-None-Match': '"other"' });
    assert.equal(changed.status, 200);
    assert.deepEqual(
      new Uint8Array(await changed.arrayBuffer()),
      method === 'GET' ? png : new Uint8Array(),
    );
    assert.equal(
      (await fetch(`/${'b'.repeat(64)}.png`, method, { 'If-None-Match': '*' })).status,
      404,
    );
  }
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
