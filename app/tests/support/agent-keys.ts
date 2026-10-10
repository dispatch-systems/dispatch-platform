import type { AgentKeyRequest } from '../../../core/platform_owner/api/index.js';
import type { fixture } from '../../../core/shell/tests/support/support.js';

type App = Awaited<ReturnType<typeof fixture>>;

/**
 * Four keys as the Agents page would hold them: read-only and operator keys, one that reaches
 * Northline only and expires in five days, and one that never expires, each used once by a
 * different client so the page shows when and from what.
 */
export async function seedAgentKeys(app: App) {
  const owner = await app.client();
  const { dsps } = (await owner.get('/api/platform/agents')).value as {
    dsps: { id: string; name: string }[];
  };
  const north = dsps.find((dsp) => dsp.name === 'Northline Logistics')!;
  const days = (n: number) => new Date(Date.now() + n * 86_400_000).toISOString();
  const key = (name: string, dsps: string[], expiresAt: string | null): AgentKeyRequest => ({
    name,
    allDsps: dsps.length === 0,
    dsps,
    access: 'read',
    expiresAt,
  });
  const keys: [AgentKeyRequest, string][] = [
    [key('Laptop – Claude Code', [], days(90)), 'claude-code/2.1.283'],
    [{ ...key('Desktop – Codex', [north.id], null), access: 'operator' }, 'codex_cli_rs/0.157.1'],
    [key('Home server – Hermes', [north.id], days(5)), 'hermes-agent/1.4.0'],
    [key('Nightly report script', [], days(180)), 'curl/8.5.0'],
  ];
  for (const [request, agent] of keys) {
    const made = await owner.post('/api/platform/agents/keys', request);
    await app.request('/api/v1/whoami', undefined, {
      authorization: `Bearer ${made.value.token}`,
      'user-agent': agent,
    });
  }
}
