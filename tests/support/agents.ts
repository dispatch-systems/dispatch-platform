import type { fixture } from './support.js';

type App = Awaited<ReturnType<typeof fixture>>;

/**
 * Four keys as the Agents page would hold them: read-only and operator keys, one with the
 * essential tools that expires in five days and one that never expires, each used once by
 * a different client so the page shows when and from what.
 */
export async function seedAgentKeys(app: App) {
  const owner = await app.client();
  const { dsps } = (await owner.get('/api/platform/agents')).value as {
    dsps: { id: string; name: string }[];
  };
  const north = dsps.find((dsp) => dsp.name === 'Northline Logistics')!;
  const days = (n: number) => new Date(Date.now() + n * 86_400_000).toISOString();
  const keys = [
    ['Laptop – Claude Code', 'read', 'full', [], days(90), 'claude-code/2.1.283'],
    ['Desktop – Codex', 'operator', 'full', [north.id], null, 'codex_cli_rs/0.157.1'],
    ['Home server – Hermes', 'read', 'essential', [north.id], days(5), 'hermes-agent/1.4.0'],
    ['Nightly report script', 'read', 'full', [], days(180), 'curl/8.5.0'],
  ] as const;
  for (const [name, access, tools, reach, expiresAt, agent] of keys) {
    const made = await owner.post('/api/platform/agents/keys', {
      name,
      allDsps: reach.length === 0,
      dsps: reach,
      access,
      tools,
      locations: false,
      expiresAt,
    });
    await app.request('/api/v1/whoami', undefined, {
      authorization: `Bearer ${made.value.token}`,
      'user-agent': agent,
    });
  }
}
