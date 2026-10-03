// What the Agents page says about keys: their access, what they read, reach, last use and
// expiry, and how an agent sets one up.
import type {
  AgentAccess,
  AgentActivity,
  AgentArea,
  AgentClient,
  AgentDsp,
  AgentDspReads,
  AgentKey,
  AgentKeyDsp,
  AgentKeyRequest,
  AgentReads,
  AgentSource,
} from '../../../../shared/contracts/index.js';
import { dateFormatter } from '../../../shell/frontend/lib/date-format.js';
import { utcDay } from '../../../shell/frontend/lib/format.js';
import { readToggles, type ReadToggle } from '../../../shell/frontend/runtime/slots.js';

export const accessLabels: Record<AgentAccess, string> = {
  read: 'Read only',
  operator: 'Operator',
};

// Each feature declares the kinds of its data agents may read, grouped under its name. They are
// read when the Agents page or the audit log first loads, which their loaders hold back until
// what every owner puts in the platform owner's slots has loaded.
const groups = readToggles();
const toggles = groups.flatMap((group) => group.toggles);
const byArea = <T>(read: (toggle: ReadToggle) => T) =>
  Object.fromEntries(toggles.map((toggle) => [toggle.id, read(toggle)])) as Record<AgentArea, T>;

/** The kinds of data a key or app may read, in the order every list shows them. */
export const agentAreas: readonly AgentArea[] = toggles.map((toggle) => toggle.id);
export const areaLabels = byArea((toggle) => toggle.label);
/** What a kind of data holds, where its label alone doesn't say. */
export const areaHints: Partial<Record<AgentArea, string>> = Object.fromEntries(
  toggles.flatMap((toggle) => (toggle.hint ? [[toggle.id, toggle.hint]] : [])),
);
/** The feature each kind of data comes from, which a DSP may have switched off. */
export const areaSources = byArea((toggle) => toggle.source);
/** The kind each comes with, for those allowed only beside another. */
export const areaWith: Partial<Record<AgentArea, AgentArea>> = Object.fromEntries(
  toggles.flatMap((toggle) => (toggle.with ? [[toggle.id, toggle.with]] : [])),
);
export const sourceLabels = Object.assign({}, ...groups.map((group) => group.sources)) as Record<
  AgentSource,
  string
>;
/** The kinds of data under the page that collects them, as the switches are grouped. `missing`
 * names a whole group a key doesn't read. */
export const areaGroups: readonly {
  label: string;
  missing: string;
  areas: readonly AgentArea[];
}[] = groups.map(({ label, missing, toggles }) => ({
  label,
  missing,
  areas: toggles.map((toggle) => toggle.id),
}));
/** A kind of data a key doesn't read, as its table row names it. */
const missingLabels = byArea((toggle) => toggle.missing);

/** What a new key or app reads: every kind but those it must opt in to, and only where the
 * DSP has the feature on. */
export const defaultReads = (): AgentReads => ({
  areas: toggles.filter((toggle) => !toggle.optIn).map((toggle) => toggle.id),
  bypass: false,
});
/** Kinds of data once each, in order. A kind that comes with another comes only with it. */
export function canonicalAreas(areas: readonly AgentArea[]) {
  const set = new Set(areas);
  for (const toggle of toggles) if (toggle.with && !set.has(toggle.with)) set.delete(toggle.id);
  return agentAreas.filter((area) => set.has(area));
}
/** `areas` with one switched on or off; switching a kind off switches off those that come
 * with it. */
export const withArea = (areas: readonly AgentArea[], area: AgentArea, on: boolean) =>
  canonicalAreas(on ? [...areas, area] : areas.filter((each) => each !== area));

const and = (names: string[]) =>
  names.length > 1 ? `${names.slice(0, -1).join(', ')} and ${names.at(-1)}` : (names[0] ?? '');
/** Small counts, as words. */
const counts = ['no', 'one', 'two', 'three', 'four'];

/** The features agents read from that a DSP has switched off, as the owner knows them: a
 * page, or only its tab when the page itself is on. */
function switchedOffNames(off: readonly AgentSource[]) {
  return areaGroups.flatMap((group) => {
    const sources = [...new Set(group.areas.map((area) => areaSources[area]))];
    const here = sources.filter((source) => off.includes(source));
    if (here.length === sources.length) return [group.label];
    return here.map((source) => sourceLabels[source]);
  });
}
/** The line under what a key reads at a DSP: which features are switched off there. */
export function switchedOffText(dsp: Pick<AgentKeyDsp, 'name' | 'switchedOff'>) {
  const names = switchedOffNames(dsp.switchedOff);
  if (!names.length) return `Every feature is on at ${dsp.name}.`;
  return `${and(names)} ${names.length === 1 ? 'is' : 'are'} switched off at ${dsp.name}.`;
}
/** What bypassing features does at a DSP with settings of its own: `who` reads the data of
 * the features switched off there, which stopped collecting when they were. */
export function bypassHere(who: string, dsp: Pick<AgentKeyDsp, 'switchedOff'>) {
  const off = switchedOffNames(dsp.switchedOff).length;
  const reads = `${who} reads every feature’s data here`;
  if (!off) return `${reads}, even if one is switched off later. It only ever reads.`;
  if (off === 1)
    return (
      `${reads}, including the one switched off. Its collection stays off, so that data ends ` +
      'on the day it was switched off. It only ever reads.'
    );
  return (
    `${reads}, including the ${counts[off]} switched off. Their collection stays off, so that ` +
    'data ends on the day each was switched off. It only ever reads.'
  );
}

/** How much a key or app reads, as its row says it: "All data" or "7 of 9 kinds", and a line
 * naming the DSPs with settings of their own, else that it bypasses features, else the one or
 * two things it doesn't read. `bypass` when any of its settings bypasses features. */
export function accessText(
  key: Pick<AgentKey, 'reads' | 'dspReads'>,
  dsps: Pick<AgentDsp, 'id' | 'name'>[],
) {
  const areas = canonicalAreas(key.reads.areas);
  const count =
    areas.length === agentAreas.length
      ? 'All data'
      : `${areas.length} of ${agentAreas.length} kinds`;
  const bypass = key.reads.bypass || key.dspReads.some((own) => own.bypass);
  if (key.dspReads.length > 1)
    return { count, note: `${key.dspReads.length} DSPs with own settings`, bypass };
  const [own] = key.dspReads;
  if (own) {
    const name = dsps.find((dsp) => dsp.id === own.dsp)?.name ?? 'A removed DSP';
    return { count, note: `${name}: own settings${own.bypass ? ', bypass on' : ''}`, bypass };
  }
  if (bypass) return { count, note: 'Bypass on', bypass };
  const missing = areaGroups.flatMap((group) => {
    const off = group.areas.filter((area) => !areas.includes(area));
    if (off.length > 1 && off.length === group.areas.length) return [group.missing];
    return off.map((area) => missingLabels[area]);
  });
  const note = missing.length && missing.length <= 2 ? `No ${missing.join(' or ')}` : '';
  return { count, note, bypass };
}

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

/** A new key's starting point: read only, every DSP, the default reads, 90 days. */
export const blankKey = (): AgentKeyRequest => ({
  name: '',
  allDsps: true,
  dsps: [],
  access: 'read',
  reads: defaultReads(),
  dspReads: [],
  expiresAt: null,
});
const ownReads = ({ dsp, areas, bypass }: AgentDspReads): AgentDspReads => ({
  dsp,
  areas: canonicalAreas(areas),
  bypass,
});
export const requestOf = (key: AgentKey): AgentKeyRequest => ({
  name: key.name,
  allDsps: key.allDsps,
  dsps: [...key.dsps].sort(),
  access: key.access,
  reads: { areas: canonicalAreas(key.reads.areas), bypass: key.reads.bypass },
  dspReads: key.dspReads.map(ownReads),
  expiresAt: key.expiresAt,
});
/** The settings of their own that go with a key: only those of the DSPs it still reaches. A
 * suspended or removed DSP isn't listed, but one the key reaches keeps its own settings, sent
 * back as they are, for when it is active again. */
export const reachedReads = (key: Pick<AgentKeyRequest, 'allDsps' | 'dsps' | 'dspReads'>) =>
  key.dspReads.filter((own) => key.allDsps || key.dsps.includes(own.dsp));
const comparable = (key: AgentKeyRequest) =>
  JSON.stringify([
    key.name,
    key.allDsps,
    [...key.dsps].sort(),
    key.access,
    canonicalAreas(key.reads.areas),
    key.reads.bypass,
    key.dspReads.map(ownReads).sort((a, b) => a.dsp.localeCompare(b.dsp)),
    key.expiresAt,
  ]);
export const sameRequest = (a: AgentKeyRequest, b: AgentKeyRequest) =>
  comparable(a) === comparable(b);

/** The apps "Connect an app" offers, in its order: each by the kind of app it is, the name it
 * goes by here, and the name it signs in with. "Other app" is any app else. */
export const connectApps = [
  { id: 'chatgpt', label: 'ChatGPT', client: 'ChatGPT' },
  { id: 'codex', label: 'Codex CLI', client: 'Codex' },
  { id: 'claude-code', label: 'Claude Code', client: 'Claude Code' },
  { id: 'hermes', label: 'Hermes', client: 'Hermes Agent' },
  { id: 'other', label: 'Other app', client: null },
] as const;
export type ConnectApp = (typeof connectApps)[number];

/** The known app a connected app is, by the name it signed in with; null for any other. */
export const knownApp = (client: Pick<AgentClient, 'name'> | null) =>
  connectApps.find((app) => app.client !== null && app.client === client?.name)?.id ?? null;

/** Whether `key` is the app being connected as `app`: that known app, or any app for "Other
 * app". */
export const connectedAs = (key: AgentKey, app: ConnectApp['id']) =>
  key.kind === 'app' && (app === 'other' || knownApp(key.client) === app);

/** How each app signs in with Dispatch, and what the owner does once it asks to (`next`). A
 * terminal app has its command, what happens next, the way from a machine with no browser, and
 * a prompt that asks the app to do it itself. */
export function signIns(origin: string) {
  const mcp = `${origin}/api/v1/mcp`;
  const next = 'Approve it on the Dispatch page that opens';
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
      label: 'Codex CLI',
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
      next: `${next}, then answer Y to turn on the tools`,
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
  return { mcp, next, terminals, bridge: `npx -y mcp-remote ${mcp}` };
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
