import { title } from '../../core/shell/frontend/lib/format.js';
import type { AuditPhrases, AuditWording } from '../../core/shell/frontend/runtime/slots.js';

// How the MCP's events read in the platform owner's audit log: the keys and apps made,
// changed and revoked, and which apps may connect.

const phrases: AuditPhrases = {
  'agent.key_created': (e, { strong }) => ['created the agent key ', strong(e.target ?? 'a key')],
  'agent.key_updated': (e, { strong }) => ['changed the agent key ', strong(e.target ?? 'a key')],
  'agent.key_revoked': (e, { strong }) => ['revoked the agent key ', strong(e.target ?? 'a key')],
  'agent.app_connected': (e, { strong }) => ['connected the app ', strong(e.target ?? 'an app')],
  'agent.app_updated': (e, { strong }) => [
    'changed the connected app ',
    strong(e.target ?? 'an app'),
  ],
  'agent.app_revoked': (e, { strong }) => [
    'revoked the connected app ',
    strong(e.target ?? 'an app'),
  ],
  'agent.pairing_opened': () => ['let apps start connecting for 10 minutes'],
  'agent.app_allowed': (e, { strong }) => ['let ', strong(e.target ?? 'an app'), ' connect'],
  'agent.app_disallowed': (e, { strong }) => [
    'stopped ',
    strong(e.target ?? 'an app'),
    ' from connecting',
  ],
  'agent.keys_revoked': (e, { strong }) => [
    'revoked ',
    strong(e.detail === '1' ? 'an agent key' : `${e.detail} agent keys`),
  ],
};

// The tools a key or app may use, and whether tools that only read come as they are added.
// Earlier key changes, from when keys chose what they read: a kind of data each, as
// `reads.timecards`, reads as its title.
const fields: Record<string, string> = {
  all_tools: 'New tools that only read',
  bypass: 'Bypass features',
  dsp_reads: 'DSP settings',
  tools: 'Tools',
  locations: 'Addresses and GPS',
};
const values: Record<string, string> = {
  read: 'Read only',
  operator: 'Operator',
  full: 'Full',
  essential: 'Essential',
  true: 'On',
  false: 'Off',
  never: 'Never',
};
const worded = ['access', 'bypass', 'tools', 'all_tools', 'locations'];
// Why Dispatch itself ended a connected app.
const appEndings: Record<string, string> = {
  replaced: 'Replaced by a new connection',
  signed_out: 'The app signed out',
  code_reused: 'Its sign-in code was used twice',
  refresh_reused: 'An old token was used again',
  token_reuse: 'An old token was used again',
};

export const wording: AuditWording = {
  phrases,
  fields,
  // A key's expiry is a date the log writes as it writes any, unless it never comes.
  value: (field, value) => {
    if (field === 'expires') return value === 'never' ? values.never : undefined;
    return worded.includes(field) || field.startsWith('reads.')
      ? (values[value] ?? value)
      : undefined;
  },
  notes: (event) => {
    const reason = event.changes.find((change) => change.field === 'reason')?.to;
    return event.action === 'agent.app_revoked' && reason
      ? [appEndings[reason] ?? title(reason)]
      : [];
  },
};
