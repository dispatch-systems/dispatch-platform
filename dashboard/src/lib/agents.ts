// What the Agents page says about keys: their access, reach, last use and expiry, and how
// an agent sets one up.
import type {
  AgentAccess,
  AgentActivity,
  AgentDsp,
  AgentKey,
  AgentKeyRequest,
  AgentTools,
} from '../../../shared/contracts/index.js';
import { dateFormatter } from './date-format.js';
import { utcDay } from './format.js';

export const accessLabels: Record<AgentAccess, string> = {
  read: 'Read only',
  operator: 'Operator',
};
export const toolLabels: Record<AgentTools, string> = {
  full: 'Full tools',
  essential: 'Essential tools',
};

const DAY = 86_400_000;
/** How long a new key lasts, as the key sheet offers it. */
export type ExpiryChoice = '7' | '30' | '90' | '365' | 'never' | 'date';
export const expiryChoices: [ExpiryChoice, string][] = [
  ['7', '7 days'],
  ['30', '30 days'],
  ['90', '90 days'],
  ['365', '1 year'],
  ['never', 'Never'],
  ['date', 'Pick a date'],
];
/** The expiry a choice means: an RFC 3339 time, or null for never. A picked date lasts
 * through that day where the viewer is. */
export function expiryOf(choice: ExpiryChoice, date: string, now = Date.now()): string | null {
  if (choice === 'never') return null;
  if (choice === 'date') return date ? new Date(`${date}T23:59:59`).toISOString() : null;
  return new Date(now + Number(choice) * DAY).toISOString();
}

const monthDay = (value: string, year: boolean) =>
  dateFormatter('en-US', {
    month: 'short',
    day: 'numeric',
    ...(year ? { year: 'numeric' } : {}),
  }).format(new Date(value));

/** Where a key stands now. A key expiring within a week asks to be looked at. */
export function keyState(key: AgentKey, now = Date.now()) {
  if (key.revokedAt) return 'revoked';
  if (!key.expiresAt) return 'never';
  const left = Date.parse(key.expiresAt) - now;
  if (left <= 0) return 'expired';
  return left <= 7 * DAY ? 'expiring' : 'active';
}
export const inUse = (key: AgentKey, now = Date.now()) =>
  !['revoked', 'expired'].includes(keyState(key, now));

/** Whole days until a key expires, rounded up. */
export const daysLeft = (key: AgentKey, now = Date.now()) =>
  key.expiresAt ? Math.ceil((Date.parse(key.expiresAt) - now) / DAY) : Infinity;

/** The key's expiry as the table shows it: "Dec 30, 2026", "Oct 6 · 5 days" or "Never". */
export function expiryText(key: AgentKey, now = Date.now()) {
  const state = keyState(key, now);
  if (state === 'revoked') return `Revoked ${monthDay(key.revokedAt!, false)}`;
  if (state === 'never') return 'Never';
  if (state === 'expired') return `Expired ${monthDay(key.expiresAt!, false)}`;
  if (state === 'expiring') {
    const days = daysLeft(key, now);
    return `${monthDay(key.expiresAt!, false)} · ${days === 1 ? '1 day' : `${days} days`}`;
  }
  return monthDay(key.expiresAt!, true);
}

/** The DSPs a key reaches: "All DSPs", or a count with their names. */
export function reachText(key: Pick<AgentKey, 'allDsps' | 'dsps'>, dsps: AgentDsp[]) {
  if (key.allDsps) return { count: 'All DSPs', names: '' };
  const names = key.dsps.map((id) => dsps.find((dsp) => dsp.id === id)?.name ?? 'A removed DSP');
  return {
    count: names.length === 1 ? '1 DSP' : `${names.length} DSPs`,
    names: names.join(', '),
  };
}

/** When a key was last used: "2 min ago", "Yesterday, 6:14 PM", "Sep 28" or "Never used". */
export function lastUsedText(value: string | null, now = Date.now()) {
  if (!value) return 'Never used';
  const when = Date.parse(value);
  const minutes = Math.round((now - when) / 60_000);
  if (minutes < 1) return 'Just now';
  if (minutes < 60) return `${minutes} min ago`;
  const day = (ms: number) => new Date(ms).toDateString();
  const clock = dateFormatter('en-US', { hour: 'numeric', minute: '2-digit' }).format(when);
  if (day(when) === day(now)) return `Today, ${clock}`;
  if (day(when) === day(now - DAY)) return `Yesterday, ${clock}`;
  return monthDay(value, new Date(when).getFullYear() !== new Date(now).getFullYear());
}

/** What the Activity tab says in place of a call: the row that marks a key or app past its
 * 10,000 recorded calls in a UTC day, after which its calls that day go unlisted. The day is
 * named as UTC counts it, since the viewer's own day ends at another time. Null for a call. */
export function activityNote(
  call: Pick<AgentActivity, 'at' | 'outcome' | 'key'>,
  now = Date.now(),
) {
  if (call.outcome !== 'capped') return null;
  return (
    `Over 10,000 calls from ${call.key.name} on ${utcDay(call.at, now)} (UTC); ` +
    'later calls that day aren’t listed.'
  );
}

const appKinds: Record<string, string> = {
  chatgpt: 'ChatGPT',
  codex: 'Codex',
  'claude-code': 'Claude Code',
  hermes: 'Hermes Agent',
  local: 'apps on this computer',
  web: 'websites and other apps',
};
/** A kind of app the owner lets connect, by its id, as a sentence names it: "this app" for
 * a missing or unknown id. */
export const appKindName = (id: string | null) =>
  (id && Object.hasOwn(appKinds, id) && appKinds[id]) || 'this app';

/** Where an agent's call went, as the Activity tab shows it: `rest:whoami` is the REST
 * endpoint whoami, `mcp:find_driver` the MCP tool find_driver. */
export function surfaceOf(surface: string) {
  const split = surface.indexOf(':');
  const via = split < 0 ? '' : surface.slice(0, split);
  const labels: Record<string, string> = { rest: 'REST', mcp: 'MCP' };
  return labels[via]
    ? { via: labels[via], name: surface.slice(split + 1) }
    : { via: '', name: surface };
}

/** A new key's starting point: read only, every tool, no addresses, 90 days. */
export const blankKey = (): AgentKeyRequest => ({
  name: '',
  allDsps: true,
  dsps: [],
  access: 'read',
  tools: 'full',
  locations: false,
  expiresAt: null,
});
export const requestOf = (key: AgentKey): AgentKeyRequest => ({
  name: key.name,
  allDsps: key.allDsps,
  dsps: [...key.dsps].sort(),
  access: key.access,
  tools: key.tools,
  locations: key.locations,
  expiresAt: key.expiresAt,
});
export const sameRequest = (a: AgentKeyRequest, b: AgentKeyRequest) =>
  JSON.stringify({ ...a, dsps: [...a.dsps].sort() }) ===
  JSON.stringify({ ...b, dsps: [...b.dsps].sort() });

/** How each app signs in with Dispatch. A terminal app has its command, what happens next,
 * the way from a machine with no browser, and a prompt that asks the app to do it itself. */
export function signIns(origin: string) {
  const mcp = `${origin}/api/v1/mcp`;
  const next = 'A Dispatch page opens — approve it there.';
  const approve = 'It opens a Dispatch page in my browser: tell me to approve it there.';
  const fallback = (login = '') =>
    `If it can't open a browser here, ${login}send me the link it prints, and when I paste back ` +
    'the address my browser ends on, enter it at its prompt or open it with curl on this machine.';
  const claude = `claude mcp add --transport http --scope user dispatch ${mcp}`;
  const codex = `codex mcp add dispatch --url ${mcp}`;
  const terminals = [
    {
      id: 'claude-code',
      label: 'Claude Code',
      command: `${claude} && claude mcp login dispatch`,
      next,
      remote: {
        lead: 'Run this instead.',
        command: `${claude} && claude mcp login dispatch --no-browser`,
      },
      prompt: [
        'Connect Claude Code to Dispatch for me.',
        `1. Run: ${claude}`,
        "2. Sign in with `claude mcp login dispatch`. It needs a real terminal; if you can't " +
          'give it one (with script or tmux, say), ask me to run it in my own terminal, or ' +
          '/mcp in a new Claude Code session.',
        `3. ${approve} ${fallback('use `claude mcp login dispatch --no-browser`, ')}`,
        '4. Then tell me to restart Claude Code to load the Dispatch tools.',
      ].join('\n'),
    },
    {
      id: 'codex',
      label: 'Codex',
      command: codex,
      next,
      remote: {
        lead: 'Run the command above and press Ctrl+C when it shows a link. Then run this.',
        command: 'codex mcp login dispatch --no-browser',
      },
      prompt: [
        'Connect Codex to Dispatch for me.',
        `1. Run: ${codex}`,
        `2. ${approve} ${fallback('stop it and run `codex mcp login dispatch --no-browser`, ')}`,
        '3. Then tell me to restart Codex to load the Dispatch tools.',
      ].join('\n'),
    },
    {
      id: 'hermes',
      label: 'Hermes',
      command: `hermes mcp add dispatch --url ${mcp} --auth oauth --connect-timeout 300`,
      next: 'A Dispatch page opens — approve it there, then answer Y to turn on the tools.',
      remote: { lead: 'Run the same command. With no browser to open, it prints a link.' },
      prompt: [
        'Connect Hermes to Dispatch for me.',
        '1. Add this under mcp_servers in ~/.hermes/config.yaml, keeping everything else:',
        '  dispatch:',
        `    url: "${mcp}"`,
        '    auth: oauth',
        `2. Run \`hermes mcp login dispatch\`. ${approve} ${fallback()}`,
        '3. Then tell me to run /reload-mcp to load the Dispatch tools.',
      ].join('\n'),
    },
  ];
  return {
    mcp,
    terminals,
    json: JSON.stringify({ mcpServers: { dispatch: { url: mcp } } }, null, 2),
    bridge: `npx -y mcp-remote ${mcp}`,
  };
}

/** How each kind of agent connects with a key: its own setup, ready to paste. */
export function setups(origin: string, token: string) {
  const mcp = `${origin}/api/v1/mcp`;
  const key = `export DISPATCH_KEY="${token}"`;
  const skill = (folder: string) =>
    [
      '# The Dispatch skill',
      `curl -fsS -H "Authorization: Bearer $DISPATCH_KEY" ${origin}/api/v1/skill \\`,
      `  --create-dirs -o ${folder}/dispatch/SKILL.md`,
    ].join('\n');
  return [
    {
      id: 'claude-code',
      label: 'Claude Code',
      text: [
        key,
        '',
        'claude mcp add --transport http --scope user dispatch \\',
        `  ${mcp} \\`,
        '  --header "Authorization: Bearer $DISPATCH_KEY"',
        '',
        skill('~/.claude/skills'),
      ].join('\n'),
    },
    {
      id: 'codex',
      label: 'Codex',
      text: [
        '# In your shell profile',
        key,
        '',
        `codex mcp add dispatch --url ${mcp} \\`,
        '  --bearer-token-env-var DISPATCH_KEY',
        '',
        skill('~/.agents/skills'),
      ].join('\n'),
    },
    {
      id: 'hermes',
      label: 'Hermes',
      text: [
        '# In your shell profile',
        key,
        '',
        '# ~/.hermes/config.yaml',
        'mcp_servers:',
        '  dispatch:',
        `    url: "${mcp}"`,
        '    headers:',
        '      Authorization: "Bearer ${DISPATCH_KEY}"',
        '',
        skill('~/.hermes/skills/operations'),
      ].join('\n'),
    },
    {
      id: 'mcp',
      label: 'Other MCP apps',
      text: JSON.stringify(
        {
          mcpServers: {
            dispatch: { type: 'http', url: mcp, headers: { Authorization: `Bearer ${token}` } },
          },
        },
        null,
        2,
      ),
    },
    {
      id: 'rest',
      label: 'REST and curl',
      text: [
        key,
        '',
        '# Ask Dispatch who the key belongs to',
        `curl -H "Authorization: Bearer $DISPATCH_KEY" ${origin}/api/v1/whoami`,
      ].join('\n'),
    },
  ] as const;
}
