import { useState } from 'react';
import { ChevronLeft, Printer } from 'lucide-react';
import { DateField, ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { ConductOverview, ConductSaved, ConductWriteup } from '../../api/index.js';
import { addWriteup, changeWriteup } from '../../api/client.js';
import { Notice } from '../notice/Notice.js';
import { usePrintNotice } from '../notice/usePrintNotice.js';
import { record } from '../tiers.js';
import { SaveChanges } from '../SaveChanges.js';
import { TierPicker } from './TierPicker.js';
import { type Draft, filled, filledOf, inputOf, missing, refill } from './draft.js';

/** The earliest a writeup may be dated: the start of the year before last. */
const earliest = (today: string) => `${Number(today.slice(0, 4)) - 2}-01-01`;

/**
 * Writing up someone, or changing a writeup not yet signed: who, the category, the date and the
 * tier, with the category's wording to edit, beside the notice as it will print.
 */
export function WriteupForm({
  overview,
  initial,
  editing,
  supervisorName,
  cancel,
  saved,
}: {
  overview: ConductOverview;
  initial: Draft;
  editing?: ConductWriteup;
  supervisorName: string;
  cancel: () => void;
  /** Called with what a save answered, once its notice has printed when it was to print. */
  saved: (saved: ConductSaved) => void;
}) {
  const [start] = useState(() =>
    refill(
      initial,
      filledOf(initial),
      editing ? undefined : filled(overview, initial.person, initial.category),
    ),
  );
  const [state, setState] = useState(start);
  // The revision the draft was read at: a later read of the writeup doesn't move it, so a save
  // over someone else's change is refused rather than overwriting it.
  const [revision] = useState(editing?.revision);
  const { draft } = state;
  const [problem, setProblem] = useState('');
  const { print, sheet } = usePrintNotice();
  const edit = (next: Partial<Draft>) => {
    setProblem('');
    setState(({ draft, before }) => {
      const changed = { ...draft, ...next };
      const chose = next.person !== undefined || next.category !== undefined;
      return chose
        ? refill(changed, before, filled(overview, changed.person, changed.category, editing?.id))
        : { draft: changed, before };
    });
  };
  const save = useAction(
    async (andPrint: boolean) => {
      const why = missing(draft);
      if (why) return setProblem(why);
      const result = editing
        ? await changeWriteup(editing.id, inputOf(draft, revision))
        : await addWriteup(inputOf(draft));
      if (andPrint) print(result.writeup, () => saved(result));
      else saved(result);
    },
    { inline: true },
  );
  const [confirming, setConfirming] = useState(false);
  // Changes leave only through Save or Discard.
  const leave = () =>
    JSON.stringify(draft) !== JSON.stringify(start.draft) ? setConfirming(true) : cancel();

  const { stood, earlier } = record(overview, draft.person, draft.category, editing?.id);
  const category = overview.categories.find((c) => c.id === draft.category);
  const people = overview.people.filter((p) => !p.left || p.code === draft.person);
  const categories = overview.categories.filter((c) => !c.retired || c.id === draft.category);
  const person = overview.people.find((p) => p.code === draft.person);
  const content = {
    personName: person?.name ?? editing?.personName ?? '',
    supervisorName: editing?.supervisorName ?? supervisorName,
    writtenOn: draft.writtenOn,
    tier: draft.tier,
    finalKind: draft.tier === 'final' && draft.final !== 'none' ? draft.final : null,
    suspension: draft.suspension,
    statement: draft.statement,
    prior: draft.prior,
    plan: draft.plan,
  };
  return (
    <>
      <button type="button" className="conduct-back" onClick={leave}>
        <ChevronLeft size={15} />
        Conduct
      </button>
      <div className="page-heading">
        <h1>{editing ? 'Edit writeup' : 'New writeup'}</h1>
        <div className="heading-actions">
          <button type="button" onClick={leave}>
            Cancel
          </button>
          <button type="button" disabled={save.busy} onClick={() => void save.run(false)}>
            Save
          </button>
          <button
            type="button"
            className="primary"
            disabled={save.busy}
            onClick={() => void save.run(true)}
          >
            <Printer size={16} />
            Save and print
          </button>
        </div>
      </div>
      <ErrorBox message={problem || save.error} />
      <div className="conduct-compose">
        <div className="conduct-fields">
          <div className="conduct-pair">
            <label className="conduct-field">
              <span className="conduct-label">Driver</span>
              <select value={draft.person} onChange={(e) => edit({ person: e.target.value })}>
                <option value="">Choose a driver</option>
                {people.map((p) => (
                  <option key={p.code} value={p.code}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="conduct-field">
              <span className="conduct-label">Category</span>
              <select value={draft.category} onChange={(e) => edit({ category: e.target.value })}>
                <option value="">Choose a category</option>
                {categories.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <div className="conduct-pair">
            <div className="conduct-field">
              <span className="conduct-label" aria-hidden="true">
                Date
              </span>
              <DateField
                label="Date"
                value={draft.writtenOn}
                min={earliest(overview.today)}
                max={overview.today}
                today={overview.today}
                onChange={(writtenOn) => edit({ writtenOn })}
              />
            </div>
            <label className="conduct-field">
              <span className="conduct-label">Supervisor</span>
              <input value={content.supervisorName} disabled />
            </label>
          </div>
          <TierPicker
            tier={draft.tier}
            suggested={
              category
                ? filled(overview, draft.person, draft.category, editing?.id)?.tier
                : undefined
            }
            earlier={earlier}
            atFinal={stood === 'final'}
            final={draft.final}
            suspension={draft.suspension}
            change={(next) => edit(next)}
          />
          <label className="conduct-field">
            <span className="conduct-label">1. Statement of the problem</span>
            <textarea
              rows={4}
              value={draft.statement}
              onChange={(e) => edit({ statement: e.target.value })}
            />
          </label>
          <label className="conduct-field">
            <span className="conduct-label">2. Prior discussion or warnings</span>
            <textarea
              rows={5}
              value={draft.prior}
              onChange={(e) => edit({ prior: e.target.value })}
            />
          </label>
          <label className="conduct-field">
            <span className="conduct-label">3. Corrective action</span>
            <textarea
              rows={4}
              value={draft.plan}
              onChange={(e) => edit({ plan: e.target.value })}
            />
          </label>
        </div>
        <div className="conduct-preview">
          <Notice content={content} marked />
        </div>
      </div>
      {sheet}
      {confirming && (
        <SaveChanges
          what="This writeup"
          saveable={!missing(draft)}
          busy={save.busy}
          // Saved, it opens the writeup.
          save={async () => {
            if (!(await save.run(false))) setConfirming(false);
          }}
          discard={cancel}
        />
      )}
    </>
  );
}
