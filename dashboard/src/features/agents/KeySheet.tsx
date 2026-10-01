import { useState } from 'react';
import { Ban } from 'lucide-react';
import type {
  AgentDsp,
  AgentKey,
  AgentKeyCreated,
  AgentKeyRequest,
} from '../../../../shared/contracts/index.js';
import { ConfirmDialog, Modal } from '../../ui/index.js';
import { useAction } from '../../app/useAction.js';
import { createAgentKey, revokeAgentKey, updateAgentKey } from '../../app/endpoints.js';
import {
  blankKey,
  expiryChoices,
  expiryOf,
  lastUsedText,
  requestOf,
  sameRequest,
  type ExpiryChoice,
} from '../../lib/agents.js';
import { dateFormatter } from '../../lib/date-format.js';

const day = (value: string | null) =>
  value
    ? dateFormatter('en-US', { month: 'short', day: 'numeric', year: 'numeric' }).format(
        new Date(value),
      )
    : 'Never';

/**
 * Makes a key, or changes one: where it reaches, what it may do, its tools and expiry.
 * Edits leave only through Save or Discard; closing the tab drops them silently.
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
    { success: existing ? 'Key updated' : undefined },
  );
  const revoke = useAction(
    async () => {
      await revokeAgentKey(existing!.id);
      changed();
    },
    { success: 'Key revoked' },
  );
  const toggle = (id: string, on: boolean) =>
    set({ dsps: on ? [...form.dsps, id] : form.dsps.filter((dsp) => dsp !== id) });
  const days = (choice: ExpiryChoice) =>
    ['7', '30', '90', '365'].includes(choice) ? ` · ${day(expiryOf(choice, ''))}` : '';
  return (
    <Modal variant="sheet" title={existing ? existing.name : 'New key'} onClose={leave}>
      <form
        className="agents-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (ready && (dirty || !existing)) void save.run();
        }}
      >
        {existing && (
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
        <fieldset>
          <legend>DSPs</legend>
          <div className="agents-choices">
            <label className="agents-choice">
              <input
                type="radio"
                name="dsps"
                checked={form.allDsps}
                onChange={() => set({ allDsps: true, dsps: [] })}
              />
              <span>
                <strong>All DSPs</strong>
                <small>Including DSPs added later</small>
              </span>
            </label>
            <label className="agents-choice">
              <input
                type="radio"
                name="dsps"
                checked={!form.allDsps}
                onChange={() => set({ allDsps: false })}
              />
              <span>
                <strong>Choose DSPs</strong>
                {!form.allDsps && (
                  <span className="agents-dsps">
                    {dsps.map((dsp) => (
                      <label key={dsp.id}>
                        <input
                          type="checkbox"
                          checked={form.dsps.includes(dsp.id)}
                          onChange={(event) => toggle(dsp.id, event.target.checked)}
                        />
                        {dsp.name}
                      </label>
                    ))}
                  </span>
                )}
              </span>
            </label>
          </div>
        </fieldset>
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
                <small>Also run collections and test connections</small>
              </span>
            </label>
          </div>
        </fieldset>
        <div className="agents-row">
          <span id="agents-tools">Tools</span>
          <div className="agents-segmented" role="group" aria-labelledby="agents-tools">
            {(['full', 'essential'] as const).map((tools) => (
              <button
                key={tools}
                type="button"
                aria-pressed={form.tools === tools}
                onClick={() => set({ tools })}
              >
                {tools === 'full' ? 'Full' : 'Essential'}
              </button>
            ))}
          </div>
        </div>
        <label className="agents-row">
          Delivery addresses and GPS
          <input
            type="checkbox"
            role="switch"
            checked={form.locations}
            onChange={(event) => set({ locations: event.target.checked })}
          />
        </label>
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
        {existing ? (
          <div className="agents-actions">
            <button type="button" className="danger" onClick={() => setRevoking(true)}>
              <Ban size={16} />
              Revoke key
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
          <p>{existing ? 'This key has unsaved changes.' : 'This key hasn’t been created yet.'}</p>
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
        <ConfirmDialog
          title={`Revoke ${existing.name}?`}
          confirm="Revoke key"
          tone="danger"
          busy={revoke.busy}
          onCancel={() => setRevoking(false)}
          onConfirm={async () => {
            if (await revoke.run()) setRevoking(false);
          }}
        >
          Anything using it loses access at once. This can’t be undone.
        </ConfirmDialog>
      )}
    </Modal>
  );
}
