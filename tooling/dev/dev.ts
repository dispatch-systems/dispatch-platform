import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { demo } from '../testing/fixture-server.js';
import { startPreview } from './dev-server.js';

// `dispatchdev shot` and `api` find the server here, for as long as it runs: it holds the link
// that signs the owner in, so only this user may read it.
const record = path.join(process.cwd(), '.runtime/dev-server.json');

type Source = { commit: string; tree: string; dirty: boolean; fingerprint: string };
function checkout() {
  const result = spawnSync('tooling/cli/dispatchdev', ['dev', 'status', '--json'], {
    cwd: process.cwd(),
    encoding: 'utf8',
  });
  if (result.status !== 0)
    throw new Error('Unable to identify the fixture source before recording it.');
  return JSON.parse(result.stdout) as { checkout: Source; backendMatches: boolean | null };
}
const before = checkout();

const app = await startPreview();
try {
  const after = checkout();
  const identity = spawnSync(
    'ps',
    ['-ww', '-p', String(process.pid), '-o', 'lstart=', '-o', 'args='],
    {
      encoding: 'utf8',
      env: { ...process.env, LC_ALL: 'C' },
    },
  );
  if (identity.status !== 0) {
    throw new Error('Unable to identify the fixture process.');
  }
  const words = identity.stdout.trim().split(/\s+/);
  fs.mkdirSync(path.dirname(record), { recursive: true, mode: 0o700 });
  const directory = path.dirname(record);
  if (fs.realpathSync(directory) !== path.join(fs.realpathSync(process.cwd()), '.runtime'))
    throw new Error('The fixture receipt directory must not be a symlink.');
  if ((fs.statSync(directory).mode & 0o077) !== 0)
    throw new Error('The fixture receipt directory must be private (mode 0700).');
  fs.rmSync(record, { force: true });
  fs.writeFileSync(
    record,
    JSON.stringify({
      version: 1,
      root: fs.realpathSync(process.cwd()),
      pid: process.pid,
      process: {
        pid: process.pid,
        started: words.slice(0, 5).join(' '),
        command: words.slice(5).join(' '),
      },
      source:
        before.backendMatches &&
        after.backendMatches &&
        JSON.stringify(before.checkout) === JSON.stringify(after.checkout)
          ? before.checkout
          : null,
      origin: app.origin,
      link: app.previewUrl,
      password: demo.password,
    }),
    { mode: 0o600 },
  );
  process.stdout.write(
    `Checkout: ${path.basename(process.cwd())}\nDevelopment fixtures: ${app.previewUrl}\nManual sign-in: ${app.origin} (${demo.email} / ${demo.password})\nTemporary state: ${app.root}\n`,
  );
  const stop = () => void app.close();
  process.once('SIGINT', stop);
  process.once('SIGTERM', stop);
  await app.exited;
} finally {
  await app.close();
  // Another run in this checkout since may have recorded itself in its place.
  try {
    const { pid } = JSON.parse(fs.readFileSync(record, 'utf8')) as { pid: number };
    if (pid === process.pid) fs.rmSync(record);
  } catch {
    // Its record is gone already.
  }
}
