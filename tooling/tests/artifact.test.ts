import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fixture } from '../../core/shell/tests/support/support.js';
import { verifyArtifact, writeManifest } from '../../tooling/build/artifact.js';
import {
  dashboardGraphSizes,
  type DashboardAssets,
} from '../../tooling/testing/dashboard-assets.js';

test(
  'installed artifact serves the complete platform directly from Rust and has no Node runtime payload',
  { skip: process.env.DISPATCH_TEST_ARTIFACT !== '1', timeout: 60000 },
  async (t) => {
    const artifact = path.resolve('.build');
    execFileSync('python3', [
      'tooling/security/check-build-paths.py',
      path.join(artifact, 'services/rust/dispatch-backend'),
    ]);
    const manifest = verifyArtifact(artifact);
    assert.equal(manifest.format, 3);
    const f = await fixture({
      seed: false,
      binary: path.join(artifact, 'services/rust/dispatch-backend'),
      env: { DISPATCH_ARTIFACT_ROOT: artifact },
    });
    t.after(f.close);
    // Host commands use the shipped executable without taking the running app's data lock.
    assert.deepEqual(JSON.parse(f.cli(['host', 'capabilities'])), {
      hostManagement: 1,
      artifactFormat: 3,
    });
    assert.equal(
      JSON.parse(f.cli(['host', 'artifact', 'verify', artifact])).digest,
      manifest.digest,
    );
    assert.equal((await f.request('/api/health')).value.release, manifest.digest);
    const owner = await f.client();
    await owner.select(owner.session.dsps[0].id);
    // The build carries its license and tells the dashboard which commit it came from.
    assert.equal(
      fs.readFileSync(path.join(artifact, 'dashboard/LICENSE.txt'), 'utf8'),
      fs.readFileSync('LICENSE', 'utf8'),
    );
    const { commit } = JSON.parse(
      fs.readFileSync(path.join(artifact, 'tooling/build-info.json'), 'utf8'),
    );
    assert.deepEqual(owner.session.source, { version: null, commit });
    assert.equal((await owner.get('/api/dsp/employees')).value.total, 0);
    assert.equal(fs.readFileSync(`/proc/${f.pid()}/task/${f.pid()}/children`, 'utf8').trim(), '');
    const html = await fetch(f.env.DISPATCH_ORIGIN + '/');
    assert.equal(html.status, 200);
    const document = await html.text();
    assert.match(document, /<div id="root">/);
    assert.equal(html.headers.get('cache-control'), 'no-store');
    const build = document.match(
      /<meta\s+name="dispatch-build"\s+content="([^"]+)"\s*\/?\s*>/,
    )?.[1];
    assert(build, 'the served document carries its build identity');
    const update = await f.request('/api/browser-update');
    assert.equal(update.status, 200);
    assert.equal(update.headers.get('cache-control'), 'no-store');
    assert.deepEqual(update.value, { build, ready: true });
    const asset = document.match(/src="(\.?\/assets\/[^"]+\.js)"/)![1]!;
    const url = new URL(asset, f.env.DISPATCH_ORIGIN!);
    const get = await fetch(url, { headers: { 'accept-encoding': 'identity' } });
    const bytes = await get.arrayBuffer();
    assert(
      bytes.byteLength < 400_000,
      'main dashboard JavaScript stays below 400 KB before compression',
    );
    const assets: DashboardAssets = JSON.parse(
      fs.readFileSync(path.join(artifact, 'tooling/build-info.json'), 'utf8'),
    ).dashboardAssets;
    const graphs = [
      { name: 'initial', entries: [], raw: 400_000, transferred: 115_000 },
      {
        name: 'DSP picker',
        entries: ['../../core/platform_owner/frontend/dsps/picker.ts'],
        raw: 410_000,
        transferred: 120_000,
      },
      {
        name: 'Settings profile',
        entries: ['../../features/settings/frontend/index.ts'],
        raw: 440_000,
        transferred: 130_000,
      },
      {
        name: 'daily Timecard',
        entries: [
          '../../features/timecard/frontend/index.ts',
          '../../features/timecard/frontend/tabs/daily/TimecardsPage.tsx',
        ],
        raw: 500_000,
        transferred: 150_000,
      },
      {
        name: 'Timecard settings',
        entries: ['../../features/timecard/frontend/settings/index.ts'],
        raw: 490_000,
        transferred: 145_000,
      },
    ];
    for (const graph of graphs) {
      const size = dashboardGraphSizes(path.join(artifact, 'dashboard'), assets, [
        'index.html',
        ...graph.entries,
      ]);
      assert(
        size.scripts.raw < graph.raw,
        `${graph.name} complete JavaScript graph exceeds ${graph.raw} bytes`,
      );
      assert(
        size.scripts.transferred < graph.transferred,
        `${graph.name} transferred JavaScript graph exceeds ${graph.transferred} bytes`,
      );
      assert(
        size.styles.raw < (graph.name === 'initial' ? 30_000 : 60_000),
        `${graph.name} complete CSS graph exceeds its budget`,
      );
      t.diagnostic(
        `${graph.name}: ${size.scripts.raw} raw / ${size.scripts.transferred} transferred JavaScript bytes`,
      );
    }
    const stylesheet = document.match(/href="(\.?\/assets\/[^"]+\.css)"/)![1]!;
    const styles = await fetch(new URL(stylesheet, f.env.DISPATCH_ORIGIN));
    assert(
      (await styles.arrayBuffer()).byteLength < 30_000,
      'initial common CSS stays below 30 KB; feature CSS loads with its route',
    );

    assert.equal(get.headers.get('cache-control'), 'public, max-age=31536000, immutable');
    const head = await fetch(url, { method: 'HEAD', headers: { 'accept-encoding': 'identity' } });
    assert.equal(head.headers.get('etag'), get.headers.get('etag'));
    assert.equal(Number(head.headers.get('content-length')), bytes.byteLength);
    assert.equal((await head.arrayBuffer()).byteLength, 0);
    const cached = await fetch(url, {
      headers: { 'if-none-match': get.headers.get('etag')!, 'accept-encoding': 'identity' },
    });
    assert.equal(cached.status, 304);
    assert.equal((await cached.arrayBuffer()).byteLength, 0);
    for (const encoding of ['br', 'gzip']) {
      const compressed = await fetch(url, { headers: { 'accept-encoding': encoding } });
      assert.equal(compressed.headers.get('content-encoding'), encoding);
      assert.equal(compressed.headers.get('vary'), 'Accept-Encoding');
      assert.deepEqual(Buffer.from(await compressed.arrayBuffer()), Buffer.from(bytes));
      assert(Number(compressed.headers.get('content-length')) < bytes.byteLength);
      assert.notEqual(compressed.headers.get('etag'), get.headers.get('etag'));
      const conditional = await fetch(url, {
        method: 'HEAD',
        headers: {
          'accept-encoding': encoding,
          'if-none-match': compressed.headers.get('etag')!,
        },
      });
      assert.equal(conditional.status, 304);
      assert.equal(conditional.headers.get('content-encoding'), encoding);
    }
    const noCompression = await fetch(url, {
      headers: { 'accept-encoding': '*;q=1, gzip;q=0, br;q=0' },
    });
    assert.equal(noCompression.headers.get('content-encoding'), null);
    const mapFile = manifest.files.find((file) =>
      /dashboard\/assets\/onboarding-map-.*\.svg$/.test(file.path),
    )!;
    assert(mapFile, 'onboarding map is shipped as an independent vector asset');
    const map = await fetch(f.env.DISPATCH_ORIGIN + mapFile.path.replace('dashboard', ''), {
      headers: { 'accept-encoding': 'br' },
    });
    assert.equal(map.headers.get('content-type'), 'image/svg+xml');
    assert.equal(map.headers.get('cache-control'), 'public, max-age=31536000, immutable');
    assert.equal(map.headers.get('content-encoding'), 'br');
    assert(
      Number(map.headers.get('content-length')) < 900_000,
      'compressed vector map stays within its initial-load budget',
    );
    const vanFile = manifest.files.find((file) =>
      /dashboard\/assets\/login-van-.*\.glb$/.test(file.path),
    );
    assert(vanFile, 'van is a separate desktop-only asset');
    const van = await fetch(f.env.DISPATCH_ORIGIN + vanFile.path.replace('dashboard', ''), {
      headers: { 'accept-encoding': 'br' },
    });
    assert.equal(van.headers.get('content-type'), 'model/gltf-binary');
    assert.equal(van.headers.get('cache-control'), 'public, max-age=31536000, immutable');
    assert.equal(van.headers.get('content-encoding'), 'br');
    assert(
      Number(van.headers.get('content-length')) < 300_000,
      'compressed van transfer stays under 300 KB',
    );
    const rendererFile = manifest.files.find((file) =>
      /dashboard\/assets\/renderer-.*\.js$/.test(file.path),
    );
    assert(rendererFile, '3D renderer is a deferred chunk');
    const renderer = await fetch(
      f.env.DISPATCH_ORIGIN + rendererFile.path.replace('dashboard', ''),
      { headers: { 'accept-encoding': 'br' } },
    );
    assert(
      Number(renderer.headers.get('content-length')) < 200_000,
      'deferred renderer transfer stays under 200 KB',
    );
    const font = await fetch(f.env.DISPATCH_ORIGIN + '/assets/inter.woff2');
    assert.equal(font.headers.get('cache-control'), 'public, no-cache');
    assert.equal((await f.request('/api/session')).headers.get('cache-control'), 'no-store');
    assert.equal(JSON.parse(f.cli(['status'])).runtime, 'rust');
    const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-bad-artifact-'));
    t.after(() => fs.rmSync(scratch, { recursive: true, force: true }));
    const candidate = path.join(scratch, 'artifact');
    for (const [name, contents] of Object.entries({
      'services/rust/dispatch-backend': '#!/bin/sh\nexit 91\n',
      'dashboard/index.html': '<div id="root"></div>',
      'tooling/build-info.json': JSON.stringify({ commit }),
    })) {
      const file = path.join(candidate, name);
      fs.mkdirSync(path.dirname(file), { recursive: true });
      fs.writeFileSync(file, contents);
    }
    writeManifest(candidate, manifest.version);
    fs.appendFileSync(path.join(candidate, 'services/rust/dispatch-backend'), 'tampered');
    assert.throws(() => verifyArtifact(candidate), /Artifact file verification failed/);
  },
);
