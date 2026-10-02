import type { R2Bucket } from '@cloudflare/workers-types';

export interface Env {
  SCREENSHOTS: R2Bucket;
}

// `npm run pr:screenshots -- publish` stores each PR screenshot as `<sha256>.png`, so a key's
// bytes never change and a response may be cached for good.
const key = /^\/([0-9a-f]{64}\.png)$/;
const missing = () => new Response('Not found', { status: 404 });
const matches = (condition: string, etag: string) =>
  condition.split(',').some((tag) => tag.trim() === '*' || tag.trim().replace(/^W\//, '') === etag);

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const name = key.exec(new URL(request.url).pathname)?.[1];
    if (!name || (request.method !== 'GET' && request.method !== 'HEAD')) return missing();
    const condition = request.headers.get('if-none-match');
    const metadataOnly = request.method === 'HEAD' || condition !== null;
    const content = metadataOnly ? null : await env.SCREENSHOTS.get(name);
    const object = metadataOnly ? await env.SCREENSHOTS.head(name) : content;
    if (!object) return missing();
    const headers = {
      'Cache-Control': 'public, max-age=31536000, immutable',
      'Content-Type': 'image/png',
      'Content-Length': String(object.size),
      ETag: object.httpEtag,
      'X-Content-Type-Options': 'nosniff',
    };
    if (condition !== null && matches(condition, object.httpEtag))
      return new Response(null, { status: 304, headers });
    if (request.method === 'HEAD') return new Response(null, { headers });
    const response = content ?? (await env.SCREENSHOTS.get(name));
    // R2 and Response share the runtime Web Stream; their separate TS libraries differ.
    return response
      ? new Response(response.body as unknown as ReadableStream<Uint8Array>, { headers })
      : missing();
  },
};
