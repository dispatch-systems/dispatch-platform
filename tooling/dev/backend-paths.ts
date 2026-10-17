import fs from 'node:fs';
import type { ProxyOptions } from 'vite';

// Where the backend answers rather than the frontend: the first part of each route's path, as
// `npm run contracts:generate` writes them from the backend's route table. `/api` is one; so
// are the paths outside it that Slack and MCP clients call at this server's own address.
const paths: string[] = JSON.parse(
  fs.readFileSync(new URL('../../app/generated/backend-paths.json', import.meta.url), 'utf8'),
);

const escaped = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/** A dev server's proxy: each of the backend's paths, and all under it, sent on by `proxy`. */
export const backendProxy = (proxy: ProxyOptions): Record<string, ProxyOptions> =>
  Object.fromEntries(paths.map((first) => [`^${escaped(first)}(?:[/?]|$)`, proxy]));
