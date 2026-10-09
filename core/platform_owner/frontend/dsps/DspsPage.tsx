import { useState } from 'react';
import { Plus } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import { useUpdateState } from '../../../shell/frontend/runtime/browser-update.js';
import { api } from '../../../shell/frontend/runtime/api.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import { usePlatformDsps } from '../../api/client.js';
import {
  hashQuery,
  navigate,
  platformHash,
  replaceHashQuery,
} from '../../../shell/frontend/runtime/navigation.js';
import {
  ConfirmDialog,
  DataState,
  Empty,
  ErrorBox,
  Header,
  Modal,
} from '../../../shell/frontend/ui/index.js';
import { DspDetail, type DspAction } from './DspDetail.js';
import { DspListPane } from './DspListPane.js';
import { open } from './open.js';
import { dspState, dspStates, stateLabels } from './status.js';

const none: DspSummary[] = [];
const requests: Record<Exclude<DspAction, 'view' | 'code'>, [string, unknown, string]> = {
  enable: ['status', { status: 'active' }, 'DSP enabled'],
  retry: ['retry', {}, 'DSP available'],
  restore: ['restore', {}, 'DSP restored'],
  remove: ['remove', {}, 'DSP removed'],
  disable: ['status', { status: 'suspended' }, 'DSP disabled'],
};

export function DspsPage() {
  const { data, error, refresh } = usePlatformDsps(10000);
  const [query, setQuery] = useUpdateState('dsp-query', ''),
    [creating, setCreating] = useState(false),
    [confirming, setConfirming] = useState<{ action: 'remove' | 'disable'; dsp: DspSummary }>(),
    [coding, setCoding] = useState<DspSummary>(),
    // The chosen DSP rides in the address, so a reload and a phone's back link keep it.
    [chosen, setChosen] = useState(() => hashQuery().get('dsp') ?? '');
  const dsps = data ?? none;
  const visible = dsps.filter((dsp) =>
    `${dsp.name} ${dsp.ownerEmail ?? ''}`.toLowerCase().includes(query.toLowerCase()),
  );
  const explicit = dsps.find((dsp) => dsp.id === chosen);
  const shown = explicit ?? visible[0];
  const select = (dsp?: DspSummary) => {
    setChosen(dsp?.id ?? '');
    if (dsp) replaceHashQuery({ dsp: dsp.id });
    else navigate(platformHash('dsps'));
  };
  const create = useAction(
    async (ownerEmail: FormDataEntryValue | null) => {
      await api('/api/platform/dsps', { ownerEmail });
      refresh();
    },
    { success: (ownerEmail) => `Invitation email queued for ${ownerEmail}` },
  );
  const setCode = useAction(
    async (dsp: DspSummary, code: string) => {
      await api(`/api/platform/dsps/${dsp.id}/code`, { code });
      setCoding(undefined);
      refresh();
    },
    { inline: true, success: (_, code) => `Short code set to ${code.toUpperCase()}` },
  );
  const change = useAction(
    async (dsp: DspSummary, action: Exclude<DspAction, 'view' | 'code'>) => {
      const [path, body] = requests[action];
      await api(`/api/platform/dsps/${dsp.id}/${path}`, body);
      setConfirming(undefined);
      refresh();
    },
    { success: (_, action) => requests[action][2] },
  );
  const act = (dsp: DspSummary, action: DspAction) => {
    if (action === 'view') open(dsp);
    else if (action === 'code') setCoding(dsp);
    else if (action === 'remove' || action === 'disable') setConfirming({ action, dsp });
    else void change.run(dsp, action);
  };
  return (
    <>
      <Header title="DSPs">
        <button className="primary" onClick={() => setCreating(true)}>
          <Plus size={17} />
          Create new DSP
        </button>
      </Header>
      <ErrorBox message={error} />
      <div className="dsp-counts" aria-label="DSP summary">
        <span className="dsp-count total">
          <strong>{dsps.length}</strong> DSPs
        </span>
        {dspStates.map((state) => (
          <span className={`dsp-count ${state}`} key={state}>
            <i />
            <strong>{dsps.filter((dsp) => dspState(dsp) === state).length}</strong>{' '}
            {stateLabels[state]}
          </span>
        ))}
      </div>
      <DataState data={data} failed={Boolean(error)} retry={refresh}>
        {() => (
          <div className={`dsps-master ${explicit ? 'chosen' : ''}`}>
            <DspListPane
              dsps={visible}
              query={query}
              setQuery={setQuery}
              selected={shown?.id}
              onSelect={select}
              refresh={refresh}
            />
            {shown ? (
              <DspDetail
                dsp={shown}
                back={() => select()}
                act={(action) => act(shown, action)}
                changed={refresh}
              />
            ) : (
              <Empty title="No DSPs found">Try another search or create your first DSP.</Empty>
            )}
          </div>
        )}
      </DataState>
      {creating && (
        <Modal title="Create new DSP" variant="sheet" onClose={() => setCreating(false)}>
          <form
            onSubmit={async (event) => {
              event.preventDefault();
              const form = new FormData(event.currentTarget);
              if (await create.run(form.get('ownerEmail'))) setCreating(false);
            }}
          >
            <label>
              Owner email
              <input
                name="ownerEmail"
                type="email"
                autoComplete="off"
                maxLength={254}
                required
                disabled={create.busy}
              />
            </label>
            <div className="form-actions">
              <button type="button" onClick={() => setCreating(false)}>
                Cancel
              </button>
              <button className="primary" disabled={create.busy}>
                {create.busy ? 'Creating…' : 'Create DSP'}
              </button>
            </div>
          </form>
        </Modal>
      )}
      {coding && (
        <Modal
          title={coding.code ? 'Change short code' : 'Set short code'}
          variant="sheet"
          onClose={() => setCoding(undefined)}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              const code = String(new FormData(event.currentTarget).get('code')).trim();
              void setCode.run(coding, code);
            }}
          >
            <label>
              Short code
              <input
                name="code"
                autoComplete="off"
                required
                minLength={2}
                maxLength={16}
                pattern="[A-Za-z0-9]{2,16}"
                title="2 to 16 letters and numbers"
                defaultValue={coding.code?.toUpperCase()}
                disabled={setCode.busy}
              />
            </label>
            <p>
              {coding.code
                ? `${coding.name}’s members sign in at the new address from then on. The old one stops working, and so do passkeys made there.`
                : `${coding.name}’s members sign in at the address it gives.`}
            </p>
            <ErrorBox message={setCode.error} />
            <div className="form-actions">
              <button type="button" onClick={() => setCoding(undefined)}>
                Cancel
              </button>
              <button className="primary" disabled={setCode.busy}>
                {setCode.busy ? 'Saving…' : 'Save'}
              </button>
            </div>
          </form>
        </Modal>
      )}
      {confirming?.action === 'remove' && (
        <ConfirmDialog
          title="Remove DSP"
          confirm="Remove DSP"
          busy={change.busy}
          onConfirm={() => void change.run(confirming.dsp, 'remove')}
          onCancel={() => setConfirming(undefined)}
        >
          Remove {confirming.dsp.name}. Access and collection will stop immediately. Existing data
          will be retained so you can restore this DSP later.
        </ConfirmDialog>
      )}
      {confirming?.action === 'disable' && (
        <ConfirmDialog
          title={`Disable ${confirming.dsp.name}?`}
          confirm="Disable DSP"
          tone="danger"
          busy={change.busy}
          onConfirm={() => void change.run(confirming.dsp, 'disable')}
          onCancel={() => setConfirming(undefined)}
        >
          Members lose access and active collections are cancelled until you enable this DSP again.
        </ConfirmDialog>
      )}
    </>
  );
}
