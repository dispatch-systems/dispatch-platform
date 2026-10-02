import { useState } from 'react';
import { BadgeCheck, LogOut, ShieldAlert } from 'lucide-react';
import type { AgentKey } from '../../../../shared/contracts/index.js';
import { DataState, Empty } from '../../ui/index.js';
import { useAgentKeys } from '../../app/endpoints.js';
import { inUse, lastUsedText, reachText, toolLabels } from '../../lib/agents.js';
import { dateFormatter } from '../../lib/date-format.js';
import { RevokeDialog } from './RevokeDialog.js';

const day = (value: string) =>
  dateFormatter('en-US', { month: 'short', day: 'numeric', year: 'numeric' }).format(
    new Date(value),
  );

/** Every app connected through "Sign in with Dispatch": what it reaches and when it was used. */
export function AppsTab() {
  const keys = useAgentKeys();
  const [showEnded, setShowEnded] = useState(false);
  const [revoking, setRevoking] = useState<AgentKey | null>(null);
  return (
    <DataState data={keys.data} error={keys.error}>
      {(data) => {
        const apps = data.keys.filter((key) => key.kind === 'app');
        const live = apps.filter((app) => inUse(app));
        const ended = apps.filter((app) => !inUse(app));
        const rows = [...live, ...(showEnded ? ended : [])];
        return (
          <div className="agents">
            {apps.length === 0 ? (
              <Empty title="No connected apps">
                An app that signs in with Dispatch shows here once you approve it.
              </Empty>
            ) : (
              <div className="table-wrap">
                <table className="agents-table">
                  <thead>
                    <tr>
                      <th>Name</th>
                      <th>App</th>
                      <th>DSPs</th>
                      <th>Tools</th>
                      <th>Addresses</th>
                      <th>Connected</th>
                      <th>Last used</th>
                      <th>
                        <span className="sr-only">Revoke</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((app) => {
                      const reach = reachText(app, data.dsps);
                      const verified = Boolean(app.client?.verified);
                      // Out of renewals, the app has to connect again from its side.
                      const signedOut = inUse(app) && app.client?.status === 'signed_out';
                      return (
                        <tr key={app.id} className={inUse(app) ? undefined : 'ended'}>
                          <td>
                            <strong>
                              <bdi>{app.name}</bdi>
                            </strong>
                            {signedOut && (
                              <small className="agents-mark warn">
                                <LogOut size={13} aria-hidden="true" />
                                Signed out — reconnect from the app
                              </small>
                            )}
                          </td>
                          <td>
                            <bdi>{app.client?.name ?? 'Unknown app'}</bdi>
                            <small className={`agents-mark${verified ? '' : ' warn'}`}>
                              {verified ? (
                                <BadgeCheck size={13} aria-hidden="true" />
                              ) : (
                                <ShieldAlert size={13} aria-hidden="true" />
                              )}
                              {verified ? 'Verified' : 'Unverified'}
                            </small>
                          </td>
                          <td>
                            {reach.count}
                            {reach.names && <small>{reach.names}</small>}
                          </td>
                          <td>{toolLabels[app.tools]}</td>
                          <td>{app.locations ? 'On' : 'Off'}</td>
                          <td>{day(app.createdAt)}</td>
                          <td>
                            {app.revokedAt
                              ? `Revoked ${day(app.revokedAt)}`
                              : lastUsedText(app.lastUsedAt)}
                            {app.lastClient && <small>{app.lastClient}</small>}
                          </td>
                          <td className="cell-end">
                            {inUse(app) && (
                              <button
                                className="agents-revoke"
                                aria-label={`Revoke ${app.name}`}
                                onClick={() => setRevoking(app)}
                              >
                                Revoke
                              </button>
                            )}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
            {ended.length > 0 && (
              <div className="agents-footer">
                <button className="text-button" onClick={() => setShowEnded(!showEnded)}>
                  {showEnded
                    ? 'Hide revoked apps'
                    : `Show ${ended.length} revoked ${ended.length === 1 ? 'app' : 'apps'}`}
                </button>
              </div>
            )}
            {revoking && (
              <RevokeDialog
                agentKey={revoking}
                noun="app"
                close={() => setRevoking(null)}
                revoked={keys.refresh}
              />
            )}
          </div>
        );
      }}
    </DataState>
  );
}
