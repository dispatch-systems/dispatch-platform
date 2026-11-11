import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import test from 'node:test';
import { createServer } from 'vite';
import { fixture } from '../testing/fixture-server.js';
import { privateFeatures } from '../build/private-features.js';
import { privateSourceGuard } from '../dev/private-sources.js';

test('standalone Vite protects custom source aliases and raw files with the real DSP session', async (t) => {
  const root = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'dispatch-private-source-'));
  const resources: {
    app?: Awaited<ReturnType<typeof fixture>>;
    ui?: Awaited<ReturnType<typeof createServer>>;
  } = {};
  t.after(async () => {
    try {
      await resources.ui?.close();
      await resources.app?.close();
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
  const write = (file: string, body: string) => {
    const target = path.join(root, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, body);
  };
  write('app/frontend/index.html', '<h1>Public</h1>');
  for (const code of ['nll', 'sum'])
    write(
      `features/custom/${code}/probe/frontend/feature.ts`,
      `export const feature = { name: '${code}_probe', label: '${code} private label' };`,
    );
  write('app/generated/features.json', '{"private":"all DSP identifiers"}');
  const outputs = [
    'Cargo.lock',
    '.build/dashboard/private-assets.json',
    '.build/dashboard/assets/private-entry.js',
    '.build-staging-probe/dashboard/private-assets.json',
    'target/debug/dispatch-backend',
    '.test-build/probe/private-input.json',
  ];
  for (const file of outputs) write(file, 'private build metadata');
  fs.symlinkSync(
    path.join(root, 'features/custom/sum/probe/frontend/feature.ts'),
    path.join(root, 'app/frontend/shortcut.ts'),
  );
  const app = await fixture();
  resources.app = app;
  const platform = await app.client();
  const member = await app.client('member@dispatch.test');
  const ui = await createServer({
    configFile: false,
    root: path.join(root, 'app/frontend'),
    logLevel: 'silent',
    plugins: [privateFeatures(root), privateSourceGuard(root, `http://127.0.0.1:${app.env.PORT}`)],
    server: { host: '127.0.0.1', port: 0, fs: { allow: [root] } },
  });
  resources.ui = ui;
  await ui.listen();
  const address = ui.httpServer!.address();
  assert(address && typeof address !== 'string');
  const origin = `http://127.0.0.1:${address.port}`;
  const send = (url: string, options: { method?: string; headers?: Record<string, string> } = {}) =>
    new Promise<{ status: number; cache: string | undefined }>((resolve, reject) => {
      const request = http.request(
        url,
        { ...options, signal: AbortSignal.timeout(10_000) },
        (response) => {
          response.on('error', reject);
          response.resume();
          response.on('end', () =>
            resolve({ status: response.statusCode!, cache: response.headers['cache-control'] }),
          );
        },
      );
      request.on('error', reject);
      request.end();
    });
  const ownerHeaders = { ...platform.headers, host: `127.0.0.1:${app.env.PORT}` };
  const anonymous = { host: member.headers.host! };
  const raw = (code: string) =>
    `/@fs${fs.realpathSync(root)}/features/custom/${code}/probe/frontend/feature.ts`;
  for (const url of [raw('nll'), '/custom/nll_probe.js', '/@id/__x00__dispatch-custom:nll_probe']) {
    for (const method of ['GET', 'HEAD']) {
      assert.equal((await send(origin + url, { method, headers: anonymous })).status, 404, url);
      assert.equal(
        (await send(origin + url, { method, headers: member.headers })).status,
        200,
        url,
      );
      assert.equal((await send(origin + url, { method, headers: ownerHeaders })).status, 200, url);
    }
  }
  for (const url of [
    raw('sum'),
    '/shortcut.ts',
    '/custom/sum_probe.js',
    '/@id/__x00__dispatch-custom:sum_probe',
    `/@fs${fs.realpathSync(root)}/app/generated/features.json`,
    ...outputs.map((file) => `/@fs${fs.realpathSync(root)}/${file}`),
  ]) {
    for (const method of ['GET', 'HEAD'])
      assert.equal((await send(origin + url, { method, headers: anonymous })).status, 404, url);
    const refused = await send(origin + url, { headers: member.headers });
    assert.equal(refused.status, 404, url);
    assert.equal(refused.cache, 'private, no-store');
    assert.equal((await send(origin + url, { headers: ownerHeaders })).status, 200, url);
  }
  const logout = await member.post('/api/auth/logout');
  assert.equal(logout.status, 200);
  assert.equal((await send(origin + raw('nll'), { headers: member.headers })).status, 404);
  assert.equal((await send(origin + '/')).status, 200);
});
