import { BrowserVerification } from './BrowserVerification.js';
import { useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { performancePolicy } from '../../shell/frontend/lib/performance-policy.js';
import { Check, Plug, RefreshCw, X } from 'lucide-react';
import type { Connection } from '../api/index.js';
import { api } from '../../shell/frontend/runtime/api.js';
import { featureLabel } from '../../shell/frontend/runtime/features.js';
import { Badge, ConfirmDialog, DataState, ErrorBox, Modal } from '../../shell/frontend/ui/index.js';
import { time, title } from '../../shell/frontend/lib/format.js';
import { messageOf } from '../../shell/frontend/lib/errors.js';
import { useAction } from '../../shell/frontend/runtime/useAction.js';
import { useFeedback } from '../../shell/frontend/runtime/feedback.js';
import { connectionUrl, useConnection } from '../api/client.js';

/** What a connection's account signs in with besides its username and password. */
export type Credentials = {
  /** What the account's username is called, and the input it takes. */
  username: { label: string; type: 'text' | 'email' };
  /** Fields before the username, given the saved connection. */
  before?: (connection: Connection | undefined) => ReactNode;
  /** Fields and notes after the password. */
  after?: ReactNode;
  /** Why the form can't be saved as it is filled in, if it can't. */
  check?: (form: FormData) => string | undefined;
  /** What the form sends before the username and password. */
  values?: (form: FormData) => Record<string, unknown>;
};

/**
 * How a card words what a connection that needs no browser asked of its provider as it
 * connected (`Connection.checks`).
 */
export type CheckWording = {
  /** Each check's name, by its id, in the order the card lists them while it connects. */
  labels: Record<string, string>;
  /** Checks listed only once they fail. */
  quiet?: readonly string[];
  /** What a failed check says under its name, by its error. */
  failure: (error: string, connection: Connection) => string;
};

/**
 * A collector's own dialog for connecting, in place of the credentials form: one that takes
 * more than a username and a password, or leads a member through what to do first.
 */
export type ConnectDialog = (dialog: {
  connection: Connection;
  development: boolean;
  /** Opened to put right what the connection's checks found, rather than to connect anew. */
  fixing: boolean;
  /** Saves the credentials and connects. `direct` says they need no browser. */
  connect: (credentials: Record<string, unknown>, direct?: boolean) => void;
  /** Checks the connection again as it is. */
  check: () => void;
  close: () => void;
}) => ReactNode;

/** A connection's card: its state, a test, its credentials, and the verification it waits for. */
export function ConnectionCard({
  development,
  provider,
  read,
  timezone,
  verification,
  credentials,
  dialog,
  checks,
  tag,
}: {
  development: boolean;
  provider: Connection['provider'];
  /** The address of the connection's state. */
  read: string;
  timezone: string;
  /** The heading over a sign-in that waits for the member's verification. */
  verification: string;
  /** What its credentials form asks for, unless it has a dialog of its own. */
  credentials?: Credentials;
  dialog?: ConnectDialog;
  /** How it words its checks, when a connection of its may need no browser. */
  checks?: CheckWording;
  /** A word beside its name, as for which way it's connected. */
  tag?: (connection: Connection) => string | undefined;
}) {
  const name = featureLabel(provider);
  const endpoint = connectionUrl(provider);
  const [poll, setPoll] = useState(performancePolicy.recoveryPollMs);
  const { data, error, refresh } = useConnection(read, poll);
  const status = data?.status;
  useEffect(() => {
    const active = status !== undefined && ['signing_in', 'needs_verification'].includes(status);
    setPoll(active ? 4000 : performancePolicy.recoveryPollMs);
  }, [status]);
  const [disconnecting, setDisconnecting] = useState(false);
  const [credentialError, setCredentialError] = useState('');
  const [saveError, setSaveError] = useState('');
  const [editing, setEditing] = useState(false),
    [fixing, setFixing] = useState(false),
    [closedVerification, setClosedVerification] = useState<string>();
  // Whether the credentials being saved need no browser, as their dialog said.
  const [direct, setDirect] = useState(false);
  // A connection that needs no browser says why it failed in its checks, on the card; any
  // other, at the top of the page too.
  const { fail } = useFeedback();
  const connect = useAction(
    async (credentials: Record<string, unknown>, onCard: boolean) => {
      fail('');
      try {
        await api(endpoint, credentials);
      } catch (error) {
        setSaveError(messageOf(error));
        if (!onCard) fail(messageOf(error));
        throw error;
      } finally {
        refresh();
      }
    },
    { success: 'Connection saved', inline: true },
  );
  const verify = useAction(
    async (code: FormDataEntryValue | null) => {
      await api(`${endpoint}/verify`, { code });
      refresh();
    },
    { success: 'Verification submitted' },
  );
  const check = useAction(
    async (onCard: boolean) => {
      fail('');
      try {
        await api(`${endpoint}/check`, {});
      } catch (error) {
        if (!onCard) fail(messageOf(error));
        throw error;
      } finally {
        refresh();
      }
    },
    { success: 'Connection checked', inline: true },
  );
  const disconnect = useAction(
    async () => {
      await api(`${endpoint}/disable`, { removeCredentials: true });
      refresh();
      setDisconnecting(false);
    },
    { success: () => `${name} disconnected` },
  );
  const saving = connect.busy,
    busy = connect.busy || disconnect.busy;
  function open(toFix: boolean) {
    setCredentialError('');
    setFixing(toFix);
    setEditing(true);
  }
  async function submit(values: Record<string, unknown>, needsNoBrowser = false) {
    setSaveError('');
    setEditing(false);
    setDirect(needsNoBrowser);
    setClosedVerification(data?.verificationSessionId);
    await connect.run(values, needsNoBrowser);
  }
  function recheck() {
    setSaveError('');
    setEditing(false);
    void check.run((data?.checks.length ?? 0) > 0);
  }
  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const refused = credentials?.check?.(form);
    if (refused) {
      setCredentialError(refused);
      return;
    }
    setCredentialError('');
    event.currentTarget.reset();
    await submit({
      ...credentials?.values?.(form),
      username: form.get('username'),
      password: form.get('password'),
    });
  }
  // While credentials that need no browser are saved, or such a connection is checked again,
  // each check waits to be asked.
  const connecting = (saving && direct) || (check.busy && (data?.checks.length ?? 0) > 0);
  const asked =
    checks && data
      ? connecting
        ? Object.keys(checks.labels).map((id) => ({ id, state: 'waiting' as const }))
        : data.status === 'ready'
          ? []
          : data.checks
      : [];
  const listed = asked.filter(
    (each) =>
      checks?.labels[each.id] && (each.state === 'failed' || !checks.quiet?.includes(each.id)),
  );
  const unmet = !saving && !check.busy && listed.some((each) => each.state === 'failed');
  return (
    <>
      <DataState data={data} error={error} retry={refresh}>
        {(data) => (
          <article className="archived-connection-card">
            <header className="connection-name">
              <h3>
                <Plug size={20} />
                {name}
              </h3>
              {!saving && tag?.(data) && <span className="connection-tag">{tag(data)}</span>}
            </header>
            <div className="archived-connection-content">
              <div role="status">
                {connecting || (listed.length > 0 && data.status === 'signing_in') ? (
                  <Badge value="signing_in">Connecting</Badge>
                ) : unmet ? (
                  <Badge value="failed">Not ready</Badge>
                ) : (
                  <Badge value={saving ? 'signing_in' : data.status} />
                )}
              </div>
              {listed.length > 0 ? (
                <ul className="connection-checks">
                  {listed.map((each) => (
                    <li key={each.id} className={each.state}>
                      {each.state === 'passed' ? (
                        <Check size={16} aria-label="Passed" />
                      ) : each.state === 'failed' ? (
                        <X size={16} aria-label="Failed" />
                      ) : (
                        <i aria-label="Waiting" />
                      )}
                      <div>
                        {checks?.labels[each.id]}
                        {each.state === 'failed' && each.error && (
                          <small>{checks?.failure(each.error, data)}</small>
                        )}
                      </div>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="muted">
                  {saving
                    ? `Signing in to ${name}…`
                    : data.status === 'ready'
                      ? `Your ${name} connection is ready to use.`
                      : data.enabled
                        ? 'Test your connection or update the saved credentials.'
                        : `Connect your ${name} account to get started.`}
                </p>
              )}
              {!saving && !unmet && (
                <ErrorBox message={saveError || (data.error ? title(data.error) : '')} />
              )}
              {!saving && data.status === 'needs_verification' && (
                <div className="verification">
                  <h3>{verification}</h3>
                  <p>
                    {data.verificationSessionId
                      ? 'Complete the verification in the browser window, then press Submit to continue.'
                      : 'Enter the verification code from your provider.'}
                  </p>
                  {data.verificationSessionId ? (
                    <button type="button" onClick={() => setClosedVerification(undefined)}>
                      Open verification window
                    </button>
                  ) : (
                    <form
                      className="inline-form"
                      onSubmit={(event) => {
                        event.preventDefault();
                        void verify.run(new FormData(event.currentTarget).get('code'));
                      }}
                    >
                      <input
                        name="code"
                        aria-label="Verification code"
                        autoComplete="one-time-code"
                        required
                        maxLength={128}
                      />
                      <button className="primary">Verify</button>
                    </form>
                  )}
                  {development && !data.verificationSessionId && (
                    <small>Synthetic fixture verification code: 123456.</small>
                  )}
                </div>
              )}

              {data.lastVerifiedAt && (
                <p className="muted">Last checked: {time(data.lastVerifiedAt, timezone)}</p>
              )}
            </div>
            <footer>
              {unmet ? (
                <>
                  <button className="primary" disabled={busy || check.busy} onClick={recheck}>
                    <RefreshCw size={16} />
                    Check again
                  </button>
                  <button disabled={busy} onClick={() => open(true)}>
                    Setup steps
                  </button>
                </>
              ) : (
                <button className="primary" disabled={busy} onClick={() => open(false)}>
                  {data.enabled ? 'Update credentials' : `Connect ${name}`}
                </button>
              )}
              {data.enabled && (
                <>
                  {!unmet && (
                    <button
                      disabled={
                        busy || data.status === 'signing_in' || data.status === 'needs_verification'
                      }
                      onClick={recheck}
                    >
                      <RefreshCw size={16} />
                      Test connection
                    </button>
                  )}
                  <button
                    className="text-button"
                    disabled={busy}
                    onClick={() => setDisconnecting(true)}
                  >
                    Disconnect
                  </button>
                </>
              )}
            </footer>
          </article>
        )}
      </DataState>
      {disconnecting && (
        <ConfirmDialog
          title={`Disconnect ${name}?`}
          confirm="Disconnect"
          busy={busy}
          onConfirm={() => {
            setSaveError('');
            void disconnect.run();
          }}
          onCancel={() => setDisconnecting(false)}
        >
          Features will lose access to this service until you reconnect. Previously collected data
          will remain available.
        </ConfirmDialog>
      )}
      {editing &&
        data &&
        dialog?.({
          connection: data,
          development,
          fixing,
          connect: (values, needsNoBrowser) => void submit(values, needsNoBrowser),
          check: recheck,
          close: () => setEditing(false),
        })}
      {editing && !dialog && credentials && (
        <Modal title={`${name} credentials`} onClose={() => setEditing(false)}>
          <p className="muted">
            Enter the account your DSP uses. Saved credentials are encrypted and are never displayed
            here.
          </p>
          {development && (
            <div className="notice">
              Development uses synthetic data. Use any test credentials; use password
              “require-verification” to exercise verification.
            </div>
          )}
          <form onSubmit={(event) => void save(event)} onInput={() => setCredentialError('')}>
            {credentials.before?.(data)}
            <label>
              {credentials.username.label}
              <input
                name="username"
                type={credentials.username.type}
                required
                maxLength={200}
                autoComplete="off"
              />
            </label>
            <label>
              Password
              <input
                name="password"
                type="password"
                required
                maxLength={256}
                autoComplete="new-password"
              />
            </label>
            {credentials.after}
            <ErrorBox message={credentialError} />
            <div className="form-actions">
              <button type="button" onClick={() => setEditing(false)}>
                Cancel
              </button>
              <button className="primary" disabled={busy}>
                {busy ? 'Saving…' : 'Save credentials'}
              </button>
            </div>
          </form>
        </Modal>
      )}
      {data?.verificationSessionId &&
        closedVerification !== data.verificationSessionId &&
        !editing &&
        !saving &&
        !disconnecting && (
          <BrowserVerification
            key={data.verificationSessionId}
            sessionId={data.verificationSessionId}
            provider={provider}
            close={() => {
              setClosedVerification(data.verificationSessionId);
              refresh();
            }}
          />
        )}
    </>
  );
}
