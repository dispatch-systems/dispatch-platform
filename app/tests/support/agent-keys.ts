import type { AgentKeys } from '../../../mcp/api/index.js';
import type { fixture } from '../../../core/shell/tests/support/support.js';

type App = Awaited<ReturnType<typeof fixture>>;

/**
 * How a key or app made with the page's first choices says what it may use: every tool there
 * is, to read, or with none yet, those to come.
 */
export async function startingTools(app: App) {
  const owner = await app.client();
  const { tools } = (await owner.get('/api/platform/agents')).value as AgentKeys;
  return { text: tools.length ? 'All tools' : 'Read tools as added', count: tools.length };
}
