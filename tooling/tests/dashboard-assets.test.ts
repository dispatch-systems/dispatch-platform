import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { brotliDecompressSync, gunzipSync } from 'node:zlib';
import { compressAssets } from '../build/compress-assets.js';
import { dashboardGraphSizes } from '../testing/dashboard-assets.js';

test('small shared chunks use lossless precompressed representations when they are smaller', async (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-small-chunk-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const file = path.join(root, 'shared.js');
  const source = Buffer.from(
    `export const labels = ${JSON.stringify(Array(12).fill('A shared label'))};`,
  );
  assert(source.length < 1024);
  fs.writeFileSync(file, source);
  await compressAssets(root);
  const br = fs.readFileSync(file + '.br');
  const gz = fs.readFileSync(file + '.gz');
  assert(br.length < source.length);
  assert(gz.length < source.length);
  assert.deepEqual(brotliDecompressSync(br), source);
  assert.deepEqual(gunzipSync(gz), source);
  assert.deepEqual(
    dashboardGraphSizes(root, { shared: { file: 'shared.js' } }, ['shared']).scripts,
    {
      raw: source.length,
      transferred: br.length,
    },
  );
});

test('critical dashboard graphs count shared dependencies and CSS once while leaving dynamic panels deferred', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-dashboard-graph-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const [file, size] of Object.entries({
    'app.js': 100,
    'route.js': 50,
    'shared.js': 80,
    'shared.js.br': 20,
    'common.css': 40,
    'route.css': 30,
    'large-panel.js': 900,
  }))
    fs.writeFileSync(path.join(root, file), Buffer.alloc(size));
  const manifest = {
    'index.html': {
      file: 'app.js',
      imports: ['shared'],
      css: ['common.css'],
      dynamicImports: ['panel'],
    },
    route: { file: 'route.js', imports: ['shared'], css: ['common.css', 'route.css'] },
    shared: { file: 'shared.js' },
    panel: { file: 'large-panel.js' },
  };
  assert.deepEqual(dashboardGraphSizes(root, manifest, ['index.html', 'route']), {
    scripts: { raw: 230, transferred: 170 },
    styles: { raw: 70, transferred: 70 },
    files: ['app.js', 'common.css', 'route.css', 'route.js', 'shared.js'],
  });
  assert.throws(() => dashboardGraphSizes(root, manifest, ['missing']), /Missing dashboard entry/);
});
