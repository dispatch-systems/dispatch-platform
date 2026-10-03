import { BrowserVerification } from './BrowserVerification.js';
import { useEffect, useState, type FormEvent, type ReactNode } from 'react';
import { performancePolicy } from '../../shell/frontend/lib/performance-policy.js';
import { Plug, RefreshCw } from 'lucide-react';
import type { Connection } from '../../../shared/contracts/index.js';
import { api } from '../../shell/frontend/runtime/api.js';
import { featureLabel } from '../../shell/frontend/runtime/features.js';
import { Badge, ConfirmDialog, DataState, ErrorBox, Modal } from '../../shell/frontend/ui/index.js';
import { time, title } from '../../shell/frontend/lib/format.js';
import { messageOf } from '../../shell/frontend/lib/errors.js';
import { useAction } from '../../shell/frontend/runtime/useAction.js';
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

/** A connection's card: its state, a test, its credentials, and the verification it waits for. */
export function ConnectionCard({
  development,
  provider,
  read,
  timezone,
  verification,
  credentials,
}: {
  development: boolean;
  provider: Connection['provider'];
  /** The address of the connection's state. */
  read: string;
  timezone: string;
  /** The heading over a sign-in that waits for the member's verification. */
  verification: string;
  credentials: Credentials;
}) {
  const name = featureLabel(provider);
  const endpoint = connectionUrl(provider);
  const [poll, setPoll] = useState(performancePolicy.recoveryPollMs);
  const { data, error, refresh } = useConnection(read, poll);
  useEffect(() => {
    const active = data && ['signing_in', 'needs_verification'].includes(data.status);
    setPoll(active ? 4000 : performancePolicy.recoveryPollMs);
  }, [data?.status]);
  const [disconnecting, setDisconnecting] = useState(false);
  const [credentialError, setCredentialError] = useState('');
  const [saveError, setSaveError] = useState('');
  const [editing, setEditing] = useState(false),
    [closedVerification, setClosedVerification] = useState<string>();
  const connect = useAction(
    async (credentials: Record<string, unknown>) => {
      try {
        await api(endpoint, credentials);
      } catch (error) {
        setSaveError(messageOf(error));
        throw error;
      } finally {
        refresh();
      }
    },
    { success: 'Connection saved' },
  );
  const verify = useAction(
    async (code: FormDataEntryValue | null) => {
      await api(`${endpoint}/verify`, { code });
      refresh();
    },
    { success: 'Verification submitted' },
  );
  const check = useAction(
    async () => {
      await api(`${endpoint}/check`, {});
      refresh();
    },
    { success: 'Connection checked' },
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
  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const refused = credentials.check?.(form);
    if (refused) {
      setCredentialError(refused);
      return;
    }
    setCredentialError('');
    setSaveError('');
    event.currentTarget.reset();
    setEditing(false);
    setClosedVerification(data?.verificationSessionId);
    await connect.run({
      ...credentials.values?.(form),
      username: form.get('username'),
      password: form.get('password'),
    });
  }
  return (
    <>
      <DataState data={data} error={error} retry={refresh}>
        {(data) => (
          <article className="archived-connection-card">
            <header>
              <h3>
                <Plug size={20} />
                {name}
              </h3>
            </header>
            <div className="archived-connection-content">
              <div role="status">
                <Badge value={saving ? 'signing_in' : data.status} />
              </div>
              <p className="muted">
                {saving
                  ? `Signing in to ${name}…`
                  : data.status === 'ready'
                    ? `Your ${name} connection is ready to use.`
                    : data.enabled
                      ? 'Test your connection or update the saved credentials.'
                      : `Connect your ${name} account to get started.`}
              </p>
              {!saving && <ErrorBox message={saveError || (data.error ? title(data.error) : '')} />}
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
              <button
                className="primary"
                disabled={busy}
                onClick={() => {
                  setCredentialError('');
                  setEditing(true);
                }}
              >
                {data.enabled ? 'Update credentials' : `Connect ${name}`}
              </button>
              {data.enabled && (
                <>
                  <button
                    disabled={
                      busy || data.status === 'signing_in' || data.status === 'needs_verification'
                    }
                    onClick={() => {
                      setSaveError('');
                      void check.run();
                    }}
                  >
                    <RefreshCw size={16} />
                    Test connection
                  </button>
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
      {editing && (
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
