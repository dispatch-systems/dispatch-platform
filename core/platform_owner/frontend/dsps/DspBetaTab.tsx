import { useState } from 'react';
import { Plus, X } from 'lucide-react';
import type { DspSummary } from '../../../accounts/api/index.js';
import type { BetaMember } from '../../api/index.js';
import {
  addBetaTesters,
  removeBetaTester,
  useDspBetaTesters,
  useDspFeatures,
} from '../../api/client.js';
import { featureCatalog } from '../../../shell/frontend/runtime/features.js';
import { switchIcon } from '../../../shell/frontend/runtime/slots.js';
import { useAction } from '../../../shell/frontend/runtime/useAction.js';
import { calendarDay } from '../../../shell/frontend/lib/format.js';
import { Empty, ErrorBox, Modal, SearchInput } from '../../../shell/frontend/ui/index.js';
import { switchLabel } from './switches.js';

// A DSP's beta testers: those of its people who see what is in beta there, and hold all of it,
// whatever their roles. Above them, what is in beta there, as its Features tab sets it.
export function DspBetaTab({ dsp }: { dsp: DspSummary }) {
  const { data, error, refresh } = useDspBetaTesters(dsp.id);
  const features = useDspFeatures(dsp.id);
  const [adding, setAdding] = useState(false);
  const inBeta = featureCatalog.filter((feature) =>
    features.data?.features.some(
      (state) => state.feature === feature.id && state.visibility === 'beta',
    ),
  );
  const off = (id: string) =>
    !features.data?.features.some((state) => state.feature === id && state.enabled);
  const testers = data?.members.filter((member) => member.testerSince) ?? [];
  const removing = useAction(
    async (member: BetaMember) => {
      await removeBetaTester(dsp.id, member.id);
      refresh();
    },
    { success: (member) => `${member.name} removed from beta testers` },
  );
  return (
    <div className="dsp-features">
      <ErrorBox message={error || features.error} />
      <section aria-label="In beta">
        <h3 className="dsp-group">In beta</h3>
        {inBeta.length ? (
          <ul className="dsp-beta-features">
            {inBeta.map((feature) => {
              const Icon = switchIcon(feature.kind === 'sub' ? feature.page : feature.id);
              return (
                <li key={feature.id}>
                  {Icon && <Icon size={14} aria-hidden="true" />}
                  {switchLabel(feature)}
                  {feature.kind === 'sub' && (
                    <small>{featureCatalog.find((page) => page.id === feature.page)?.label}</small>
                  )}
                  {off(feature.id) && <small>Off</small>}
                </li>
              );
            })}
          </ul>
        ) : (
          <p className="dsp-beta-none">Nothing in beta</p>
        )}
      </section>
      <section aria-label="Testers">
        <div className="dsp-beta-head">
          <h3 className="dsp-group">Testers</h3>
          <button className="primary" disabled={!data} onClick={() => setAdding(true)}>
            <Plus size={16} aria-hidden="true" />
            Add tester
          </button>
        </div>
        {data && !testers.length ? (
          <Empty title="No testers" />
        ) : (
          <ul className="dsp-feature-list">
            {testers.map((member) => (
              <li key={member.id} className="dsp-feature-row dsp-tester">
                <span className="dsp-person">
                  <strong>{member.name}</strong>
                  <small>{member.email}</small>
                </span>
                <small>{member.role}</small>
                <small>{calendarDay(member.testerSince ?? '')}</small>
                <button
                  className="icon-button"
                  aria-label={`Remove ${member.name}`}
                  title="Remove"
                  disabled={removing.busy}
                  onClick={() => void removing.run(member)}
                >
                  <X size={16} aria-hidden="true" />
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
      {adding && data && (
        <AddTesters
          dsp={dsp}
          members={data.members}
          onClose={() => setAdding(false)}
          onDone={() => {
            setAdding(false);
            refresh();
          }}
        />
      )}
    </div>
  );
}

// Chooses some of the DSP's people to add; those who test already show as testers.
function AddTesters({
  dsp,
  members,
  onClose,
  onDone,
}: {
  dsp: DspSummary;
  members: readonly BetaMember[];
  onClose: () => void;
  onDone: () => void;
}) {
  const [query, setQuery] = useState('');
  const [chosen, setChosen] = useState<ReadonlySet<string>>(new Set());
  const words = query.trim().toLowerCase();
  const listed = members.filter(
    (member) => !words || `${member.name} ${member.email}`.toLowerCase().includes(words),
  );
  const adding = useAction(
    async () => {
      await addBetaTesters(dsp.id, [...chosen]);
      onDone();
    },
    {
      success: () =>
        chosen.size === 1 ? 'Beta tester added' : `${chosen.size} beta testers added`,
    },
  );
  const choose = (id: string, on: boolean) =>
    setChosen((was) => {
      const next = new Set(was);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  return (
    <Modal title="Add beta tester" onClose={onClose}>
      <div className="dsp-add-testers">
        <SearchInput
          label="Search members"
          placeholder="Search members"
          value={query}
          onChange={setQuery}
        />
        <ul className="dsp-tester-choices" aria-label="Members">
          {listed.map((member) => (
            <li key={member.id}>
              <label>
                <input
                  type="checkbox"
                  checked={Boolean(member.testerSince) || chosen.has(member.id)}
                  disabled={Boolean(member.testerSince)}
                  onChange={(event) => choose(member.id, event.target.checked)}
                />
                <span className="dsp-person">
                  <strong>{member.name}</strong>
                  <small>{member.email}</small>
                </span>
                <small>{member.testerSince ? 'Tester' : member.role}</small>
              </label>
            </li>
          ))}
        </ul>
      </div>
      <div className="form-actions">
        <button type="button" onClick={onClose}>
          Cancel
        </button>
        <button
          className="primary"
          disabled={!chosen.size || adding.busy}
          onClick={() => void adding.run()}
        >
          Add
        </button>
      </div>
    </Modal>
  );
}
