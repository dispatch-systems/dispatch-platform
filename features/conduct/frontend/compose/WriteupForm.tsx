import { useEffect, useRef, useState } from 'react';
import { ChevronLeft, Printer } from 'lucide-react';
import { DateField, ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { ConductOverview, ConductSaved, ConductWriteup } from '../../api/index.js';
import { addWriteup, changeWriteup, useConductContext } from '../../api/client.js';
import { NoticePaper } from '../notice/NoticePaper.js';
import { useSaveChanges } from '../useSaveChanges.js';
import { DriverPicker } from './DriverPicker.js';
import { TemplateSection } from './TemplateSection.js';
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
  saved: (saved: ConductSaved, print?: boolean) => void;
}) {
  const [start] = useState(initial);
  const [state, setState] = useState({ draft: start, before: filledOf(start) });
  const [revision] = useState(editing?.revision);
  const { draft } = state;
  const [problem, setProblem] = useState('');
  const fields = useRef<HTMLFieldSetElement>(null);
  const category = overview.categories.find((c) => c.id === draft.category);
  const context = useConductContext(draft.person, draft.category, draft.writtenOn, editing?.id);
  const baseline = useConductContext(
    editing ? start.person : '',
    editing ? start.category : '',
    start.writtenOn,
    editing?.id,
  );
  const baselineReady = useRef(false);
  const suggestion = filled(category, context.data);
  const suggestionKey = JSON.stringify(suggestion);
  const previousSuggestion = useRef('');
  useEffect(() => {
    if (!suggestion || (editing && !baseline.data) || previousSuggestion.current === suggestionKey)
      return;
    previousSuggestion.current = suggestionKey;
    const base =
      editing && !baselineReady.current
        ? filled(
            overview.categories.find((c) => c.id === start.category),
            baseline.data,
          )
        : undefined;
    baselineReady.current = true;
    setState((current) => {
      const before = base ?? current.before;
      if (
        editing &&
        current.draft.person === start.person &&
        current.draft.category === start.category &&
        current.draft.writtenOn === start.writtenOn
      )
        return { ...current, before };
      return refill(current.draft, before, suggestion);
    });
  }, [suggestion, suggestionKey, editing, start, baseline.data, overview.categories]);
  const edit = (next: Partial<Draft>) => {
    setProblem('');
    setState((current) => ({ ...current, draft: { ...current.draft, ...next } }));
  };
  const save = useAction(
    async (andPrint: boolean) => {
      const why = missing(draft);
      if (why) {
        setProblem(why);
        const invalid =
          (!draft.person
            ? fields.current?.querySelector<HTMLElement>('[role="combobox"]')
            : !draft.category
              ? fields.current?.querySelector<HTMLElement>('select')
              : draft.tier === 'final' && draft.final === 'suspension' && !draft.suspension.trim()
                ? fields.current?.querySelector<HTMLElement>('[aria-label="Suspension length"]')
                : undefined) ??
          fields.current?.querySelector<HTMLElement>('[data-conduct-blank][data-empty]') ??
          [...(fields.current?.querySelectorAll<HTMLTextAreaElement>('textarea') ?? [])].find(
            (field) => !field.value.trim() || [...field.value].length > 4000,
          );
        (
          invalid ??
          fields.current?.querySelector<HTMLElement>(!draft.person ? '[role="combobox"]' : 'select')
        )?.focus();
        return;
      }
      if (!context.data) {
        return;
      }
      const result = editing
        ? await changeWriteup(editing.id, inputOf(draft, revision))
        : await addWriteup(inputOf(draft));
      saved(result, andPrint);
    },
    { inline: true },
  );
  const { leave, asking } = useSaveChanges({
    what: 'This writeup',
    busy: save.busy,
    dirty: JSON.stringify(draft) !== JSON.stringify(start),
    saveable: !save.busy && !missing(draft) && Boolean(context.data),
    // Saved, it opens the writeup, so leaving goes no further.
    save: async () => {
      await save.run(false);
      return false;
    },
  });

  const stood = context.data?.standing;
  const earlier = context.data?.earlier ?? [];
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
      <button
        type="button"
        className="conduct-back"
        disabled={save.busy}
        onClick={() => leave(cancel)}
      >
        <ChevronLeft size={15} />
        Conduct
      </button>
      <div className="page-heading">
        <h1>{editing ? 'Edit writeup' : 'New writeup'}</h1>
        <div className="heading-actions">
          <button type="button" disabled={save.busy} onClick={() => leave(cancel)}>
            Cancel
          </button>
          <button
            type="button"
            disabled={save.busy || (!missing(draft) && !context.data)}
            onClick={() => void save.run(false)}
          >
            Save
          </button>
          <button
            type="button"
            className="primary"
            disabled={save.busy || (!missing(draft) && !context.data)}
            onClick={() => void save.run(true)}
          >
            <Printer size={16} />
            Save and print
          </button>
        </div>
      </div>
      <ErrorBox message={problem || save.error || context.error} />
      {context.error && (
        <button type="button" onClick={context.refresh}>
          Retry loading
        </button>
      )}
      <div className="conduct-compose">
        <fieldset ref={fields} className="conduct-fields" disabled={save.busy}>
          <div className="conduct-pair">
            <label className="conduct-field">
              <span className="conduct-label">Driver</span>
              <DriverPicker
                people={people}
                value={draft.person}
                change={(person) => edit({ person })}
              />
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
            suggested={suggestion?.tier}
            earlier={earlier}
            atFinal={stood === 'final'}
            final={draft.final}
            suspension={draft.suspension}
            change={(next) => edit(next)}
          />
          <TemplateSection
            label="1. Statement of the problem"
            value={draft.statement}
            change={(statement) => edit({ statement })}
          />
          <TemplateSection
            label="2. Prior discussion or warnings"
            value={draft.prior}
            change={(prior) => edit({ prior })}
          />
          <TemplateSection
            label="3. Corrective action"
            value={draft.plan}
            change={(plan) => edit({ plan })}
          />
        </fieldset>
        <NoticePaper content={content} marked preview />
      </div>
      {asking}
    </>
  );
}
