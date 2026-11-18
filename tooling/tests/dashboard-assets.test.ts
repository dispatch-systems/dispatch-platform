import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { brotliDecompressSync, gunzipSync } from 'node:zlib';
import { compressAssets } from '../build/compress-assets.js';
import { dashboardGraphSizes } from '../testing/dashboard-assets.js';
import { assets, assetSizes, overBudget, pageSizeReport } from '../testing/page-budgets.js';

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

test('page sizes name each graph over its budget, by how much and where main stood, and warn when thin', () => {
  const graph = (name: string, scripts: number, transferred: number, styles: number) => ({
    graph: { name, entries: [], raw: 100_000, transferred: 20_000, styles: 10_000 },
    size: {
      scripts: { raw: scripts, transferred },
      styles: { raw: styles, transferred: styles },
      files: [],
    },
  });
  const sizes = (graphs: ReturnType<typeof graph>[]) => ({ graphs, assets: [] });
  const roomy = sizes([graph('Home', 90_000, 15_000, 5_000)]);
  assert.equal(overBudget(roomy), false);
  assert.deepEqual(pageSizeReport(roomy), [
    'Home: JavaScript 90,000/100,000 (10,000 left), transferred 15,000/20,000 (5,000 left), CSS 5,000/10,000 (5,000 left)',
  ]);
  // A size at its budget is over it, as the artifact test asserts each stays under.
  assert.equal(overBudget(sizes([graph('Home', 100_000, 15_000, 5_000)])), true);
  // Under 1% left warns, and thin compressed headroom says hashed names alone move it.
  const [, ...thin] = pageSizeReport(sizes([graph('Home', 99_500, 19_900, 5_000)]));
  assert.equal(thin.length, 3);
  assert.match(thin[0]!, /^Warning: Home's JavaScript has 500 bytes left/);
  assert.match(thin[1]!, /^Warning: Home's transferred JavaScript has 100 bytes left/);
  assert.match(thin[2]!, /hashed file names/);
  const change = sizes([
    graph('Home', 90_000, 20_984, 5_000),
    graph('Settings', 90_000, 15_000, 10_050),
  ]);
  assert.equal(overBudget(change), true);
  const report = (main: Parameters<typeof pageSizeReport>[1]) => pageSizeReport(change, main);
  const [, , homeOver, homeMain, settingsOver, settingsMain] = report(
    sizes([graph('Home', 90_000, 19_900, 5_000), graph('Settings', 90_000, 15_000, 10_200)]),
  );
  assert.match(
    homeOver!,
    /Home's transferred JavaScript is 20,984 bytes, 984 over its budget of 20,000/,
  );
  assert.match(homeMain!, /^ {2}main: 19,900 bytes, 100 left: main is at the edge/);
  assert.match(settingsOver!, /Settings's CSS is 10,050 bytes, 50 over its budget of 10,000/);
  assert.match(settingsMain!, /^ {2}main: 10,200 bytes, already 200 over/);
  assert.match(
    report(sizes([graph('Home', 90_000, 15_000, 5_000)]))[3]!,
    /5,000 left: this change adds 5,984/,
  );
  assert.match(report('not measured, as its packages differ')[3]!, /main: not measured/);
});

test('single files are held to their own budgets, as sent, and one a build lacks to none', (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dashboard-files-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, 'assets'));
  fs.writeFileSync(path.join(root, 'index.html'), '<script src="./assets/index-a1.js"></script>');
  fs.writeFileSync(path.join(root, 'assets/index-a1.js'), 'x'.repeat(1_000));
  // The van is sent as its Brotli file, the map as itself, having none worth writing.
  fs.writeFileSync(path.join(root, 'assets/login-van-b2.glb'), 'v'.repeat(5_000));
  fs.writeFileSync(path.join(root, 'assets/login-van-b2.glb.br'), 'v'.repeat(400));
  fs.writeFileSync(path.join(root, 'assets/route-map-c3.svg'), 'm'.repeat(700));
  const sized = Object.fromEntries(assetSizes(root).map(({ asset, size }) => [asset.name, size]));
  assert.deepEqual(sized, { 'main script': 1_000, map: 700, van: 400, renderer: undefined });
  const over = assets.map((asset) => ({
    asset,
    size: asset.name === 'van' ? asset.budget + 10 : undefined,
  }));
  const report = pageSizeReport({ graphs: [], assets: over });
  assert.equal(overBudget({ graphs: [], assets: over }), true);
  assert.ok(report.includes('renderer:    not in this build'), report.join('\n'));
  assert.ok(
    report.some((line) => /van's transferred size is 300,010 bytes, 10 over/.test(line)),
    report.join('\n'),
  );
});
