import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { build as viteBuild } from 'vite';
import { writeManifest } from './artifact.js';
import { compressAssets } from './compress-assets.js';
import { replaceBuild } from './build-output.js';

const root = process.cwd(),
  out = path.join(root, '.build'),
  // For browser tests on this machine only: the debug backend, built through the cache in seconds
  // where a release build takes minutes. The merge queue tests the release build.
  debug = process.argv.includes('--debug');
if (fs.existsSync(path.join(root, '.runtime')))
  throw new Error(
    'Build in an isolated checkout; the installed runtime is managed by the updater.',
  );
const manifest = await replaceBuild(out, async (staging) => {
  // The backend and the support worker, from one Cargo build or the cache that keeps them together.
  execFileSync('tooling/cli/dispatchdev', debug ? ['build'] : ['build', '--release'], {
    stdio: 'inherit',
  });
  const metadata = JSON.parse(
    execFileSync('cargo', ['metadata', '--no-deps', '--format-version=1', '--locked'], {
      encoding: 'utf8',
    }),
  );
  fs.mkdirSync(path.join(staging, 'services/rust'), { recursive: true });
  const packaged = (name: string) => {
    const binary = path.join(staging, 'services/rust', name);
    fs.copyFileSync(
      path.join(metadata.target_directory, debug ? 'debug' : 'release', name),
      binary,
    );
    return binary;
  };
  const backend = packaged('dispatch-backend'),
    worker = packaged('dispatch-support-worker');
  const support = path.join(staging, 'services/support-worker');
  fs.mkdirSync(support, { recursive: true });
  fs.cpSync(path.join(root, 'services/support-worker/hermes'), path.join(support, 'hermes'), {
    recursive: true,
    filter: (source) => !source.includes('__pycache__') && !source.endsWith('.pyc'),
  });
  fs.copyFileSync(
    path.join(root, 'ops/launchers/support-collection.py'),
    path.join(support, 'support-collection.py'),
  );
  fs.copyFileSync(
    path.join(root, 'ops/launchers/browseros-release.json'),
    path.join(support, 'browseros-release.json'),
  );
  fs.copyFileSync(
    path.join(root, 'ops/systemd/dispatch-support-worker.service'),
    path.join(support, 'dispatch-support-worker.service'),
  );
  // Stripped as the release profile strips its own: a debug build's debug information names this
  // machine's compiler.
  if (debug) execFileSync('strip', [backend, worker], { stdio: 'inherit' });
  execFileSync('python3', ['tooling/security/check-build-paths.py', backend, worker], {
    stdio: 'inherit',
  });
  await viteBuild({
    build: {
      outDir: path.join(staging, 'dashboard'),
      manifest: true,
    },
  });
  // Test shared modules and route CSS too. Build metadata stays outside the served dashboard.
  const assetsFile = path.join(staging, 'dashboard/.vite/manifest.json');
  const dashboardAssets = JSON.parse(fs.readFileSync(assetsFile, 'utf8'));
  fs.unlinkSync(assetsFile);
  fs.rmdirSync(path.join(staging, 'dashboard/.vite'));
  await compressAssets(path.join(staging, 'dashboard/assets'));
  // Every copy of the build carries its license. Installed updaters accept new files only under
  // dashboard/, and the server does not serve this one.
  fs.copyFileSync(path.join(root, 'LICENSE'), path.join(staging, 'dashboard/LICENSE.txt'));
  fs.mkdirSync(path.join(staging, 'tooling'));
  const commit = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  fs.writeFileSync(
    path.join(staging, 'tooling/build-info.json'),
    JSON.stringify({ commit, hostManagement: 1, supportWorker: 2, dashboardAssets }) + '\n',
  );
  writeManifest(staging, JSON.parse(fs.readFileSync('package.json', 'utf8')).version);
});
process.stdout.write(
  `Built ${manifest.version}${debug ? ' with the debug backend' : ''}: ${manifest.digest}\n${manifest.files.length} verified files in ${out}\n`,
);
