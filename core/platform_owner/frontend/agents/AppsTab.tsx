import { useState } from 'react';
import { Pencil, Plus, SlidersHorizontal } from 'lucide-react';
import type { AgentKey } from '../../../../shared/contracts/index.js';
import { Badge, DataState, Empty } from '../../../shell/frontend/ui/index.js';
import { useAgentKeys } from '../../api/client.js';
import { accessText, expiryText, inUse, knownApp, lastUsedText, reachText } from './agents.js';
import { AllowedApps } from './AllowedApps.js';
import { AppIcon } from './AppIcon.js';
import { ConnectDialog } from './ConnectDialog.js';
import { KeySheet } from './KeySheet.js';
import { RevokeDialog } from './RevokeDialog.js';

/** Every app connected through "Sign in with Dispatch": what it reaches and reads and when it
 * was used, with the way to change one, to connect another and to choose which apps may. */
export function AppsTab() {
  const keys = useAgentKeys();
  const [showEnded, setShowEnded] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const [revoking, setRevoking] = useState<AgentKey | null>(null);
  const [connecting, setConnecting] = useState(false);
  const [choosing, setChoosing] = useState(false);
  const connect = (
    <button className="primary" onClick={() => setConnecting(true)}>
      <Plus size={16} />
      Connect an app
    </button>
  );
  return (
    <DataState data={keys.data} error={keys.error} retry={keys.refresh}>
      {(data) => {
        const apps = data.keys.filter((key) => key.kind === 'app');
        const live = apps.filter((app) => inUse(app));
        const ended = apps.filter((app) => !inUse(app));
        const rows = [...live, ...(showEnded ? ended : [])];
        const edited = live.find((app) => app.id === editing);
        return (
          <div className="agents">
            {apps.length === 0 ? (
              <Empty title="No apps connected yet" action={connect} />
            ) : (
              <>
                <div className="table-toolbar agents-apps-toolbar">{connect}</div>
                <div className="table-wrap">
                  <table className="agents-table">
                    <thead>
                      <tr>
                        <th>Name</th>
                        <th>App</th>
                        <th>DSPs</th>
                        <th>Access</th>
                        <th>Last used</th>
                        <th>
                          <span className="sr-only">Edit or revoke</span>
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {rows.map((app) => {
                        const reach = reachText(app, data.dsps);
                        const access = accessText(app, data.dsps);
                        // Out of renewals, the app has to connect again from its side.
                        const signedOut = inUse(app) && app.client?.status === 'signed_out';
                        return (
                          <tr key={app.id} className={inUse(app) ? undefined : 'ended'}>
                            <td>
                              <span className="agents-app-name">
                                {/* Only recognized published metadata gets an app logo. */}
                                <AppIcon app={app.client?.known ? knownApp(app.client) : null} />
                                <span>
                                  <strong>
                                    <bdi>{app.name}</bdi>
                                  </strong>
                                  {signedOut && (
                                    <Badge value="signed_out">
                                      Signed out — reconnect from the app
                                    </Badge>
                                  )}
                                </span>
                              </span>
                            </td>
                            <td>
                              <span className="agents-named">
                                <bdi>{app.client?.name ?? 'Unknown app'}</bdi>
                                <Badge
                                  value={app.client?.known ? 'known_metadata' : 'app_provided'}
                                >
                                  {app.client?.known ? 'Known metadata' : 'App-provided metadata'}
                                </Badge>
                              </span>
                            </td>
                            <td>
                              {reach.count}
                              {reach.names && <small>{reach.names}</small>}
                            </td>
                            <td>
                              {access.count}
                              {access.note && (
                                <small className={access.bypass ? 'agents-bypass' : undefined}>
                                  {access.note}
                                </small>
                              )}
                            </td>
                            <td>
                              {/* Once it has ended: "Revoked Oct 2" or "Expired Oct 2". */}
                              {inUse(app) ? lastUsedText(app.lastUsedAt) : expiryText(app)}
                              {app.lastClient && <small>{app.lastClient}</small>}
                            </td>
                            <td className="cell-end">
                              {inUse(app) && (
                                <span className="agents-row-actions">
                                  <button
                                    aria-label={`Edit ${app.name}`}
                                    onClick={() => setEditing(app.id)}
                                  >
                                    <Pencil size={14} aria-hidden="true" />
                                    Edit
                                  </button>
                                  <button
                                    className="agents-revoke"
                                    aria-label={`Revoke ${app.name}`}
                                    onClick={() => setRevoking(app)}
                                  >
                                    Revoke
                                  </button>
                                </span>
                              )}
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
              </>
            )}
            <div className="agents-footer">
              <button className="text-button agents-quiet" onClick={() => setChoosing(true)}>
                <SlidersHorizontal size={14} aria-hidden="true" />
                Choose which apps may connect
              </button>
              {ended.length > 0 && (
                <button className="text-button" onClick={() => setShowEnded(!showEnded)}>
                  {showEnded
                    ? 'Hide revoked and expired apps'
                    : `Show ${ended.length} revoked or expired ${ended.length === 1 ? 'app' : 'apps'}`}
                </button>
              )}
            </div>
            {edited && (
              <KeySheet
                key={edited.id}
                dsps={data.dsps}
                existing={edited}
                close={() => setEditing(null)}
                created={() => {}}
                changed={() => {
                  setEditing(null);
                  keys.refresh();
                }}
              />
            )}
            {revoking && (
              <RevokeDialog
                agentKey={revoking}
                noun="app"
                close={() => setRevoking(null)}
                revoked={keys.refresh}
              />
            )}
            {connecting && (
              <ConnectDialog
                connected={keys.refresh}
                close={() => {
                  setConnecting(false);
                  keys.refresh();
                }}
              />
            )}
            {choosing && <AllowedApps close={() => setChoosing(false)} />}
          </div>
        );
      }}
    </DataState>
  );
}
