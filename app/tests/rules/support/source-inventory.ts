import { featureLocations } from '../../../../tooling/shared/features.js';
import fs from 'node:fs';
import path from 'node:path';
import { files, root } from './repo.js';

// These rules inspect source on disk, including ignored source files. Keep that scope beside
// the Git inventory: generated or temporary code must not escape frontend isolation or audit
// wording checks just because Git ignores its directory.
const sourceRoots = ['tooling', 'services', 'app', 'core', 'collectors', 'features', 'mcp'];
const source = /\.(tsx?|m?js|rs)$/;
const found = new Set(
  files.filter(
    (file) =>
      source.test(file) && sourceRoots.some((directory) => file.startsWith(`${directory}/`)),
  ),
);
for (const directory of sourceRoots)
  for (const name of fs.readdirSync(path.join(root, directory), {
    recursive: true,
    encoding: 'utf8',
  }))
    if (source.test(name)) found.add(path.posix.join(directory, name.split(path.sep).join('/')));
export const sourceFiles: readonly string[] = [...found].sort();

// Directory discovery stays on disk too: an empty or ignored frontend is still a frontend.
const owners = [
  'app',
  ...featureLocations(root).map((feature) => feature.dir),
  ...['core', 'collectors'].flatMap((top) =>
    fs.readdirSync(path.join(root, top)).map((name) => path.join(top, name)),
  ),
  'mcp',
];
/** Each owner's frontend, and each of its integrations' (`integrations/<name>/frontend`). */
export const frontends = owners
  .flatMap((directory) => {
    const integrations = path.join(directory, 'integrations');
    const own = fs.existsSync(path.join(root, integrations))
      ? fs.readdirSync(path.join(root, integrations))
      : [];
    return [
      path.join(directory, 'frontend'),
      ...own.map((name) => path.join(integrations, name, 'frontend')),
    ];
  })
  .filter((directory) => fs.existsSync(path.join(root, directory)));
/** Only an owner's api/client.ts is frontend code; its other API contracts are not. */
export const clients = owners
  .map((directory) => path.join(directory, 'api', 'client.ts'))
  .filter((file) => fs.existsSync(path.join(root, file)));
