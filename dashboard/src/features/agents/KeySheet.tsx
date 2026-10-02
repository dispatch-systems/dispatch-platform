import { useEffect, useId, useRef, useState } from 'react';
import { Ban } from 'lucide-react';
import type {
  AgentKey,
  AgentKeyCreated,
  AgentKeyDsp,
  AgentKeyRequest,
} from '../../../../shared/contracts/index.js';
import { Badge, Modal } from '../../ui/index.js';
import { useAction } from '../../app/useAction.js';
import { createAgentKey, updateAgentKey } from '../../app/endpoints.js';
import {
  blankKey,
  expiryChoices,
  expiryOf,
  knownApp,
  lastUsedText,
  reachedReads,
  requestOf,
  sameRequest,
  type ExpiryChoice,
} from '../../lib/agents.js';
import { dateFormatter } from '../../lib/date-format.js';
import { AppIcon } from './AppIcon.js';
import { DspList, DspView } from './DspSettings.js';
import { bypassHint, ReadChoices, SwitchBox } from './KeyChoices.js';
import { RevokeDialog } from './RevokeDialog.js';

const day = (value: string | null) =>
  value
    ? dateFormatter('en-US', { month: 'short', day: 'numeric', year: 'numeric' }).format(
        new Date(value),
      )
    : 'Never';

/**
 * Makes a key, or changes a key or a connected app: where it reaches, what it may do and read,
 * each DSP's own settings, and a key's expiry. A DSP's settings open in place of the rest, and
 * are saved with them. Edits leave only through Save or Discard; closing the tab drops them
 * silently.
 */
export function KeySheet({
  dsps,
  existing,
  close,
  created,
  changed,
}: {
  dsps: AgentKeyDsp[];
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
  // The DSP whose settings are open.
  const [viewing, setViewing] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [revoking, setRevoking] = useState(false);
  const set = (change: Partial<AgentKeyRequest>) =>
    setForm((current) => ({ ...current, ...change }));
  const expiresAt = expiry === 'keep' ? (existing?.expiresAt ?? null) : expiryOf(expiry, date);
  // A DSP no longer reached takes its own settings with it.
  const request = {
    ...form,
    name: form.name.trim(),
    dspReads: reachedReads(form, dsps),
    expiresAt,
  };
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

  // A DSP's settings open at the top, on the way back; leaving them returns to their Edit
  // button, where the rest was scrolled to.
  const editId = useId();
  const pane = useRef<HTMLFormElement>(null);
  const shown = useRef<string | null>(null);
  const scrolled = useRef(0);
  const view = (dsp: string) => {
    scrolled.current = pane.current?.parentElement?.scrollTop ?? 0;
    setViewing(dsp);
  };
  useEffect(() => {
    const left = shown.current;
    if (left === viewing) return;
    shown.current = viewing;
    const body = pane.current?.parentElement;
    if (viewing) {
      body?.scrollTo({ top: 0 });
      pane.current?.closest('[role="dialog"]')?.querySelector<HTMLElement>('button')?.focus();
    } else if (left) {
      body?.scrollTo({ top: scrolled.current });
      document.getElementById(`${editId}${left}`)?.focus({ preventScroll: true });
    }
  }, [viewing, editId]);
  const viewed = dsps.find((dsp) => dsp.id === viewing);
  const app = existing?.kind === 'app' ? existing : null;
  return (
    <Modal
      variant="sheet"
      title={viewed ? viewed.name : existing ? existing.name : 'New key'}
      onClose={leave}
      onBack={viewed ? () => setViewing(null) : undefined}
    >
      {viewed ? (
        <DspView
          ref={pane}
          dsp={viewed}
          form={form}
          set={set}
          noun={noun}
          done={() => setViewing(null)}
        />
      ) : (
        <form
          ref={pane}
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
          <DspList
            dsps={dsps}
            form={form}
            set={set}
            noun={noun}
            edit={view}
            editId={(dsp) => `${editId}${dsp}`}
          />
          {/* An app only ever reads. */}
          {!app && (
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
          )}
          <ReadChoices
            legend="What it can read"
            hint="Every DSP uses these, unless you give it its own settings."
            areas={form.reads.areas}
            set={(areas) => set({ reads: { ...form.reads, areas } })}
          />
          <SwitchBox
            tone="bypass"
            label="Bypass features"
            hint={bypassHint}
            on={form.reads.bypass}
            set={(bypass) => set({ reads: { ...form.reads, bypass } })}
          />
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
      )}
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
