import test from 'node:test';
import assert from 'node:assert/strict';
import { ApiError, api, credentials, wordedApi } from '../../frontend/runtime/api.js';
import { dataCache } from '../../frontend/runtime/data-cache.js';
import { readSettings, saveSettings } from '../../../settings/api/client.js';
import { sectionResolver } from '../../../settings/api/identities.js';
import type { SettingsAnswer } from '../../../settings/api/index.js';

// The server answers every call with the error code given, as it answers a refused write.
const refusing = (t: test.TestContext, code: string) =>
  t.mock.method(
    globalThis,
    'fetch',
    async () =>
      new Response(JSON.stringify({ error: code, message: code.replaceAll('_', ' ') }), {
        status: 409,
        headers: { 'x-request-id': 'request-1' },
      }),
  );

test("an owner's own error codes read as it words them, and every other as before", async (t) => {
  const api = wordedApi({ example_taken: 'Another example already uses this name.' });
  refusing(t, 'example_taken');
  await assert.rejects(api('/api/dsp/example', { name: 'One' }), {
    name: 'Error',
    code: 'example_taken',
    message: 'Another example already uses this name.',
    status: 409,
    requestId: 'request-1',
  });
  t.mock.restoreAll();
  // Core's own wording, for a code the owner doesn't word.
  refusing(t, 'rate_limited');
  const refused = await api('/api/dsp/example', {}).catch((error: unknown) => error);
  assert.ok(refused instanceof ApiError);
  assert.equal(refused.message, 'Too many attempts. Wait a few minutes and try again.');
});

test("an error a proxy answers with a page of its own reads as a refused request, not as the page's code", async (t) => {
  // Cloudflare answers a server's 502 with its own page.
  t.mock.method(
    globalThis,
    'fetch',
    async () =>
      new Response('<!DOCTYPE html><title>502 Bad gateway</title>', {
        status: 502,
        headers: { 'content-type': 'text/html', 'x-request-id': 'request-2' },
      }),
  );
  await assert.rejects(wordedApi({})('/api/dsp/example', {}), {
    code: 'request_failed',
    message: 'The request could not be completed.',
    status: 502,
    requestId: 'request-2',
  });
});

test('Settings aliases normalize before raw prefetch and repeated save cache replacement', async (t) => {
  t.after(() => credentials(''));
  credentials('fixture-csrf', 'fixture-view', { id: 'fixture-dsp', code: 'nll' });
  const section = {
    id: 'dispatch_phone.display',
    page: 'display',
    label: 'Display',
    order: 0,
    scope: 'member',
    custom: true,
    collection: null,
    reader: null,
    fields: [],
    values: { sound: false, notifications: false },
    revision: 7,
    sources: [],
    keywords: [],
  };
  let held = structuredClone(section);
  t.mock.method(
    globalThis,
    'fetch',
    async (_url: string | URL | Request, options?: RequestInit) => {
      if (options?.method === 'POST') {
        const save = JSON.parse(options.body as string) as {
          revision: number;
          values: typeof held.values;
        };
        assert.equal(save.revision, held.revision);
        held = { ...held, values: save.values, revision: held.revision + 1 };
        return Response.json({ section: held, reopen: false });
      }
      return Response.json({ pages: [{ id: 'display' }], sections: [held] });
    },
  );
  // This is the raw API/cache path used by prefetch, without a Settings hook or page.
  await dataCache.read('/api/dsp/settings', (signal) =>
    api<SettingsAnswer>('/api/dsp/settings', undefined, signal),
  );
  let current = readSettings()!.sections[0]!;
  assert.equal(current.id, 'communications.display');
  assert.deepEqual(current.values, { sound: false, notifications: false });
  assert.equal(current.revision, 7);
  for (const sound of [true, false]) {
    await saveSettings('dispatch_phone.display', {
      values: { sound, notifications: false },
      revision: current.revision,
    });
    current = readSettings()!.sections[0]!;
    assert.equal(current.id, 'communications.display');
    assert.equal(current.revision, held.revision);
  }
  assert.equal(current.revision, 9);
  assert.equal(readSettings()!.sections.length, 1);
});

test('section identity families preserve orientation and tenant eligibility', () => {
  const bridge = sectionResolver([
    { id: 'old.display', aliases: ['new.display'], owner: null },
    { id: 'nll_old.display', aliases: ['nll_new.display'], owner: 'nll' },
  ]);
  const future = sectionResolver([
    { id: 'new.display', aliases: ['old.display'], owner: null },
    { id: 'nll_new.display', aliases: ['nll_old.display'], owner: 'nll' },
  ]);
  for (const code of ['nll', 'sum', null, undefined]) {
    for (const id of ['old.display', 'new.display']) {
      assert.equal(bridge(future(id, code), code), 'old.display');
      assert.equal(future(bridge(id, code), code), 'new.display');
    }
    assert.equal(
      bridge('nll_new.display', code),
      code === 'nll' ? 'nll_old.display' : 'nll_new.display',
    );
    assert.equal(
      future('nll_old.display', code),
      code === 'nll' ? 'nll_new.display' : 'nll_old.display',
    );
  }
  assert.equal(bridge('unrecognized.display', 'nll'), 'unrecognized.display');
});
