import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { dashboardGraphSizes } from '../testing/dashboard-assets.js';

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
