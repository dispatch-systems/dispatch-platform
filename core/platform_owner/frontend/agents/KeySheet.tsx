import { useState } from 'react';
import { Ban } from 'lucide-react';
import type { AgentDsp, AgentKey, AgentKeyCreated, AgentKeyRequest } from '../../api/index.js';
import { Badge, Modal } from '../../../shell/frontend/ui/index.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import { createAgentKey, updateAgentKey } from '../../api/client.js';
import {
  blankKey,
  expiryChoices,
  expiryOf,
  knownApp,
  lastUsedText,
  requestOf,
  sameRequest,
  type ExpiryChoice,
} from './agents.js';
import { dateFormatter } from '../../../shell/frontend/lib/date-format.js';
import { AppIcon } from './AppIcon.js';
import { DspReach } from './KeyChoices.js';
import { RevokeDialog } from './RevokeDialog.js';

const day = (value: string | null) =>
  value
    ? dateFormatter('en-US', { month: 'short', day: 'numeric', year: 'numeric' }).format(
        new Date(value),
      )
    : 'Never';

/**
 * Makes a key, or changes a key or a connected app: where it reaches, what it may do, and a
 * key's expiry. Edits leave only through Save or Discard; closing the tab drops them silently.
 */
export function KeySheet({
  dsps,
  existing,
  close,
  created,
  changed,
}: {
  dsps: AgentDsp[];
  existing?: AgentKey;
  close: () => void;
  created: (made: AgentKeyCreated) => void;
  changed: () => void;
}) {
  const noun = existing?.kind === 'app' ? 'app' : 'key';
  const start = existing ? requestOf(existing) : blankKey();
  const [form, setForm] = useState<AgentKeyRequest>(start);
  const [expiry, setExpiry] = useState<ExpiryChoice | 'keep'>(existing ? 'keep' : '90');
  const [date, setDate] = useState('');
  const [confirming, setConfirming] = useState(false);
  const [revoking, setRevoking] = useState(false);
  const set = (change: Partial<AgentKeyRequest>) =>
    setForm((current) => ({ ...current, ...change }));
  const expiresAt = expiry === 'keep' ? (existing?.expiresAt ?? null) : expiryOf(expiry, date);
  const request = { ...form, name: form.name.trim(), expiresAt };
  const dirty =
    !sameRequest({ ...request, expiresAt: start.expiresAt }, start) ||
    expiry !== (existing ? 'keep' : '90');
  const ready =
    request.name.length > 0 &&
    (request.allDsps || request.dsps.length > 0) &&
    (expiry !== 'date' || Boolean(date));
  const leave = () => (dirty ? setConfirming(true) : close());
  const save = useAction(
    async () => {
      if (existing) {
        await updateAgentKey(existing.id, request);
        changed();
      } else created(await createAgentKey(request));
    },
    { success: existing ? (noun === 'app' ? 'App updated' : 'Key updated') : undefined },
  );
  const days = (choice: ExpiryChoice) =>
    ['7', '30', '90', '365'].includes(choice) ? ` · ${day(expiryOf(choice, ''))}` : '';

  const app = existing?.kind === 'app' ? existing : null;
  return (
    <Modal variant="sheet" title={existing ? existing.name : 'New key'} onClose={leave}>
      <form
        className="agents-form agents-sheet"
        onSubmit={(event) => {
          event.preventDefault();
          if (ready && (dirty || !existing)) void save.run();
        }}
      >
        {app ? (
          <dl className="agents-facts">
            <dt>App</dt>
            <dd className="agents-named">
              {/* Only recognized published metadata gets an app logo. */}
              <AppIcon app={app.client?.known ? knownApp(app.client) : null} />
              <bdi>{app.client?.name ?? 'Unknown app'}</bdi>
              <Badge value={app.client?.known ? 'known_metadata' : 'app_provided'}>
                {app.client?.known ? 'Known metadata' : 'App-provided metadata'}
              </Badge>
            </dd>
            <dt>Connected</dt>
            <dd>{day(app.createdAt)}</dd>
            <dt>Last used</dt>
            <dd>
              {lastUsedText(app.lastUsedAt)}
              {app.lastClient && ` · ${app.lastClient}`}
            </dd>
          </dl>
        ) : (
          existing && (
            <dl className="agents-facts">
              <dt>Key</dt>
              <dd className="agents-mono">…{existing.hint}</dd>
              <dt>Created</dt>
              <dd>{day(existing.createdAt)}</dd>
              <dt>Last used</dt>
              <dd>
                {lastUsedText(existing.lastUsedAt)}
                {existing.lastClient && ` · ${existing.lastClient}`}
              </dd>
            </dl>
          )
        )}
        <label>
          Name
          <input
            name="name"
            required
            maxLength={80}
            placeholder="Laptop – Claude Code"
            value={form.name}
            onChange={(event) => set({ name: event.target.value })}
          />
        </label>
        <DspReach dsps={dsps} form={form} set={set} />
        {/* Existing Operator keys retain their setting; new keys only read. */}
        {!app && existing?.access === 'operator' && (
          <fieldset>
            <legend>Access</legend>
            <div className="agents-choices two">
              <label className="agents-choice">
                <input
                  type="radio"
                  name="access"
                  checked={form.access === 'read'}
                  onChange={() => set({ access: 'read' })}
                />
                <span>
                  <strong>Read only</strong>
                  <small>Look up any data</small>
                </span>
              </label>
              <label className="agents-choice">
                <input
                  type="radio"
                  name="access"
                  checked={form.access === 'operator'}
                  onChange={() => set({ access: 'operator' })}
                />
                <span>
                  <strong>Operator</strong>
                  <small>Read access; collection controls unavailable</small>
                </span>
              </label>
            </div>
          </fieldset>
        )}
        {!app && (
          <div className="agents-expiry">
            <label>
              Expires
              <select
                value={expiry}
                onChange={(event) => setExpiry(event.target.value as ExpiryChoice | 'keep')}
              >
                {existing && <option value="keep">No change · {day(existing.expiresAt)}</option>}
                {expiryChoices.map(([choice, label]) => (
                  <option key={choice} value={choice}>
                    {label}
                    {days(choice)}
                  </option>
                ))}
              </select>
            </label>
            {expiry === 'date' && (
              <input
                type="date"
                aria-label="Expiry date"
                required
                value={date}
                onChange={(event) => setDate(event.target.value)}
              />
            )}
          </div>
        )}
        {existing ? (
          <div className="agents-actions">
            <button type="button" className="danger" onClick={() => setRevoking(true)}>
              <Ban size={16} />
              Revoke {noun}
            </button>
            <button className="primary" disabled={!ready || !dirty || save.busy}>
              Save changes
            </button>
          </div>
        ) : (
          <div className="form-actions">
            <button type="button" onClick={leave}>
              Cancel
            </button>
            <button className="primary" disabled={!ready || save.busy}>
              Create key
            </button>
          </div>
        )}
      </form>
      {confirming && (
        <Modal title="Save changes?" dismissible={false} onClose={() => {}}>
          <p>
            {existing ? `This ${noun} has unsaved changes.` : 'This key hasn’t been created yet.'}
          </p>
          <div className="form-actions">
            <button type="button" onClick={close}>
              Discard changes
            </button>
            <button
              type="button"
              className="primary"
              disabled={!ready || save.busy}
              onClick={async () => {
                if (!(await save.run())) setConfirming(false);
              }}
            >
              Save changes
            </button>
          </div>
        </Modal>
      )}
      {revoking && existing && (
        <RevokeDialog
          agentKey={existing}
          noun={noun}
          close={() => setRevoking(false)}
          revoked={changed}
        />
      )}
    </Modal>
  );
}
