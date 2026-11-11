import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { build } from 'vite';
import { privateFeatures } from '../build/private-features.js';

test('the dashboard build keeps custom entries, lazy code, styles and resources in their owner audience', async (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dispatch-private-build-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const write = (file: string, content: string) => {
    const target = path.join(root, file);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, content);
  };
  write('app/frontend/index.html', '<script type="module" src="/public.ts"></script>');
  write('app/frontend/shared.ts', 'export const shared = "shared-runtime";');
  write(
    'app/frontend/public.ts',
    `import { shared } from './shared'; document.title = shared;
     window.openOwner = () => import(${JSON.stringify(path.join(root, 'core/platform_owner/api/runtime.ts'))});`,
  );
  write('core/platform_owner/api/runtime.ts', 'export const label = "PLATFORM_ONLY_LABEL";');
  for (const [owner, name] of [
    ['a11', 'alpha'],
    ['b22', 'beta'],
  ] as const) {
    const dir = `features/custom/${owner}/${name}/frontend`;
    write(
      `${dir}/feature.ts`,
      `import './private.css';
       import picture from './private.svg';
       import { shared } from '../../../../../app/frontend/shared';
       export const feature = { name: '${owner}_${name}', label: '${name.toUpperCase()}_PRIVATE_LABEL', picture, shared, page: () => import('./page') };`,
    );
    write(`${dir}/page.ts`, `import './lazy.css'; export const page = '${name}_private_page';`);
    write(`${dir}/private.css`, `.${name}-private { color: red; }`);
    write(`${dir}/lazy.css`, `.${name}-lazy-private { color: blue; }`);
    write(
      `${dir}/private.svg`,
      `<svg xmlns="http://www.w3.org/2000/svg"><text>${name}_private_resource</text></svg>`,
    );
  }
  // Backend-only custom features still have a private entry for their catalog metadata.
  fs.mkdirSync(path.join(root, 'features/custom/a11/records'), { recursive: true });
  const out = path.join(root, 'out');
  await build({
    configFile: false,
    root: path.join(root, 'app/frontend'),
    base: './',
    logLevel: 'silent',
    plugins: [privateFeatures(root)],
    build: { outDir: out, assetsInlineLimit: 0, minify: false },
  });
  const privateAssets = JSON.parse(
    fs.readFileSync(path.join(out, 'private-assets.json'), 'utf8'),
  ) as Record<string, { owner?: string; platform?: boolean; feature?: string; styles?: string[] }>;
  const entries = Object.entries(privateAssets).filter(([, asset]) => asset.feature);
  assert.deepEqual(entries.map(([, asset]) => asset.feature).sort(), [
    'a11_alpha',
    'a11_records',
    'b22_beta',
  ]);
  for (const [file, entry] of entries) {
    assert.ok(file.startsWith(`/assets/custom-${entry.owner}-`), file);
    if (entry.feature === 'a11_records') continue;
    assert.ok(entry.styles?.length, `missing styles for ${entry.feature}`);
    for (const style of entry.styles!) assert.equal(privateAssets[style]?.owner, entry.owner);
  }
  const markers = [
    [
      'a11',
      'ALPHA_PRIVATE_LABEL',
      'alpha_private_page',
      'alpha-private',
      'alpha-lazy-private',
      'alpha_private_resource',
    ],
    [
      'b22',
      'BETA_PRIVATE_LABEL',
      'beta_private_page',
      'beta-private',
      'beta-lazy-private',
      'beta_private_resource',
    ],
  ];
  for (const [owner, ...secrets] of markers) {
    for (const secret of secrets) {
      const files = fs
        .readdirSync(out, { recursive: true, encoding: 'utf8' })
        .filter(
          (file) =>
            fs.statSync(path.join(out, file)).isFile() &&
            fs.readFileSync(path.join(out, file), 'utf8').includes(secret!),
        );
      assert.ok(files.length, `fixture marker ${secret} disappeared`);
      for (const file of files)
        assert.equal(privateAssets[`/${file}`]?.owner, owner, `${secret} escaped into ${file}`);
    }
  }
  for (const file of fs.readdirSync(out, { recursive: true, encoding: 'utf8' })) {
    if (!fs.statSync(path.join(out, file)).isFile() || file === 'private-assets.json') continue;
    const content = fs.readFileSync(path.join(out, file), 'utf8');
    if (!privateAssets[`/${file}`]) {
      assert.doesNotMatch(file, /a11|b22/);
      assert.doesNotMatch(content, /a11|b22/);
    }
    if (content.includes('PLATFORM_ONLY_LABEL'))
      assert.equal(privateAssets[`/${file}`]?.platform, true, file);
  }
  write(
    'app/frontend/public.ts',
    `import { feature } from '../../features/custom/a11/alpha/frontend/feature'; document.title = feature.label;`,
  );
  await assert.rejects(
    build({
      configFile: false,
      root: path.join(root, 'app/frontend'),
      plugins: [privateFeatures(root)],
      logLevel: 'silent',
      build: { outDir: path.join(root, 'rejected'), minify: false },
    }),
    /imports an asset outside its audience/,
  );
});
