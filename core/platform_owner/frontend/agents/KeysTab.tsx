import { useState } from 'react';
import { ChevronRight, Clock, Plus, Power, TriangleAlert } from 'lucide-react';
import type { AgentAccess, AgentKey, AgentKeyCreated } from '../../api/index.js';
import { ConfirmDialog, DataState, Empty, SearchInput } from '../../../shell/frontend/ui/index.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import { revokeAllAgentKeys, useAgentKeys } from '../../api/client.js';
import {
  accessLabels,
  daysLeft,
  expiryText,
  inUse,
  keyState,
  lastUsedText,
  reachText,
} from './agents.js';
import { KeyReady } from './KeyReady.js';
import { KeySheet } from './KeySheet.js';
import { UsingKeys } from './UsingKeys.js';

const accessTones: Record<AgentAccess, string> = {
  read: 'access-read',
  operator: 'access-operator',
};

function Expiry({ agentKey }: { agentKey: AgentKey }) {
  const state = keyState(agentKey);
  const text = expiryText(agentKey);
  if (state === 'never')
    return (
      <span className="agents-warn" title="This key never expires">
        <TriangleAlert size={14} aria-hidden="true" />
        {text}
      </span>
    );
  if (state === 'expiring')
    return (
      <span className="agents-warn">
        <Clock size={14} aria-hidden="true" />
        {text}
      </span>
    );
  return <>{text}</>;
}

/** Every key: what it may do and read, where it reaches, when it was last used and when it
 * ends; and, folded away, what an agent needs to use one. */
export function KeysTab() {
  const keys = useAgentKeys();
  const [query, setQuery] = useState('');
  const [showEnded, setShowEnded] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  const [created, setCreated] = useState<AgentKeyCreated | null>(null);
  const [revokingAll, setRevokingAll] = useState(false);
  const revokeAll = useAction(
    async () => {
      await revokeAllAgentKeys();
      keys.refresh();
    },
    { success: 'Every key was revoked' },
  );
  return (
    <>
      <DataState data={keys.data} error={keys.error} retry={keys.refresh}>
        {(data) => {
          // Apps have a tab of their own.
          const all = data.keys.filter((key) => key.kind === 'key');
          const live = all.filter((key) => inUse(key));
          const ended = all.filter((key) => !inUse(key));
          const wanted = query.trim().toLocaleLowerCase('en-US');
          const rows = [...live, ...(showEnded ? ended : [])].filter(
            (key) => !wanted || key.name.toLocaleLowerCase('en-US').includes(wanted),
          );
          const editing = all.find((key) => key.id === open);
          return (
            <div className="agents">
              <div className="table-toolbar">
                <SearchInput
                  label="Search keys"
                  placeholder="Search keys"
                  value={query}
                  onChange={setQuery}
                />
                <button className="primary" onClick={() => setOpen('new')}>
                  <Plus size={16} />
                  New key
                </button>
              </div>
              {live
                .filter((key) => keyState(key) === 'expiring')
                .map((key) => (
                  <div className="notice agents-notice" key={key.id}>
                    <TriangleAlert size={16} aria-hidden="true" />
                    <span>
                      <strong>{key.name}</strong> expires in{' '}
                      {daysLeft(key) === 1 ? '1 day' : `${daysLeft(key)} days`}.
                    </span>
                    <button onClick={() => setOpen(key.id)}>Extend</button>
                  </div>
                ))}
              {all.length === 0 ? (
                <Empty title="No keys yet">
                  Make one key for each agent or device, so you can revoke one without the rest.
                </Empty>
              ) : (
                <div className="table-wrap">
                  <table className="agents-table">
                    <thead>
                      <tr>
                        <th>Name</th>
                        <th>Access</th>
                        <th>DSPs</th>
                        <th>Last used</th>
                        <th>Expires</th>
                        <th>
                          <span className="sr-only">Open</span>
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {rows.map((key) => {
                        const reach = reachText(key, data.dsps);
                        return (
                          <tr key={key.id} className={inUse(key) ? undefined : 'ended'}>
                            <td>
                              <button className="agents-name" onClick={() => setOpen(key.id)}>
                                <strong>{key.name}</strong>
                                <small className="agents-mono">…{key.hint}</small>
                              </button>
                            </td>
                            <td>
                              <span className={`agents-tag ${accessTones[key.access]}`}>
                                {accessLabels[key.access]}
                              </span>
                            </td>
                            <td>
                              {reach.count}
                              {reach.names && <small>{reach.names}</small>}
                            </td>
                            <td>
                              {lastUsedText(key.lastUsedAt)}
                              {key.lastClient && <small>{key.lastClient}</small>}
                            </td>
                            <td>
                              <Expiry agentKey={key} />
                            </td>
                            <td className="cell-end">
                              <button
                                aria-label={`Open ${key.name}`}
                                onClick={() => setOpen(key.id)}
                              >
                                <ChevronRight size={16} />
                              </button>
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              )}
              {all.length > 0 && (
                <div className="agents-footer">
                  {ended.length > 0 ? (
                    <button className="text-button" onClick={() => setShowEnded(!showEnded)}>
                      {showEnded
                        ? 'Hide revoked and expired keys'
                        : `Show ${ended.length} revoked or expired ${ended.length === 1 ? 'key' : 'keys'}`}
                    </button>
                  ) : (
                    <span />
                  )}
                  {live.length > 0 && (
                    <button className="danger" onClick={() => setRevokingAll(true)}>
                      <Power size={16} />
                      Revoke everything
                    </button>
                  )}
                </div>
              )}
              {open === 'new' && (
                <KeySheet
                  dsps={data.dsps}
                  close={() => setOpen(null)}
                  created={(made) => {
                    setOpen(null);
                    setCreated(made);
                    keys.refresh();
                  }}
                  changed={() => {}}
                />
              )}
              {editing && (
                <KeySheet
                  key={editing.id}
                  dsps={data.dsps}
                  existing={editing}
                  close={() => setOpen(null)}
                  created={() => {}}
                  changed={() => {
                    setOpen(null);
                    keys.refresh();
                  }}
                />
              )}
              {created && <KeyReady created={created} close={() => setCreated(null)} />}
              {revokingAll && (
                <ConfirmDialog
                  title="Revoke every key?"
                  confirm="Revoke everything"
                  tone="danger"
                  busy={revokeAll.busy}
                  onCancel={() => setRevokingAll(false)}
                  onConfirm={async () => {
                    if (await revokeAll.run()) setRevokingAll(false);
                  }}
                >
                  Every agent using a key, and every connected app, loses access at once. You’ll
                  need new keys, and to connect the apps again.
                </ConfirmDialog>
              )}
            </div>
          );
        }}
      </DataState>
      {/* How to use a key needs no list, so it stays even when the list can't load. */}
      <UsingKeys />
    </>
  );
}
