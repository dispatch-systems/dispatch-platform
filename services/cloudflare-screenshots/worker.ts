import type { R2Bucket } from '@cloudflare/workers-types';

export interface Env {
  SCREENSHOTS: R2Bucket;
}

// `npm run pr:screenshots -- publish` stores each PR screenshot as `<sha256>.png`, so a key's
// bytes never change and a response may be cached for good.
const key = /^\/([0-9a-f]{64}\.png)$/;
const missing = () => new Response('Not found', { status: 404 });

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const name = key.exec(new URL(request.url).pathname)?.[1];
    if (!name || (request.method !== 'GET' && request.method !== 'HEAD')) return missing();
    const object = await env.SCREENSHOTS.get(name);
    if (!object) return missing();
    return new Response(request.method === 'HEAD' ? null : await object.arrayBuffer(), {
      headers: {
        'Cache-Control': 'public, max-age=31536000, immutable',
        'Content-Type': 'image/png',
        ETag: object.httpEtag,
        'X-Content-Type-Options': 'nosniff',
      },
    });
  },
};
