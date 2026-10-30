import fs from 'node:fs';
import path from 'node:path';
import { demo } from '../testing/fixture-server.js';
import { startPreview } from './dev-server.js';

// `dispatchdev shot` and `api` find the server here, for as long as it runs: it holds the link
// that signs the owner in, so only this user may read it.
const record = path.join(process.cwd(), '.runtime/dev-server.json');

const app = await startPreview();
fs.mkdirSync(path.dirname(record), { recursive: true, mode: 0o700 });
fs.rmSync(record, { force: true });
fs.writeFileSync(
  record,
  JSON.stringify({
    pid: process.pid,
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
await app.close();
// Another run in this checkout since may have recorded itself in its place.
try {
  const { pid } = JSON.parse(fs.readFileSync(record, 'utf8')) as { pid: number };
  if (pid === process.pid) fs.rmSync(record);
} catch {
  // Its record is gone already.
}
