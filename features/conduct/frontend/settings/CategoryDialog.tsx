import { useState } from 'react';
import { DataState, ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type {
  CategoryInput,
  ConductCategory,
  ConductOverview,
  ConductPerson,
} from '../../api/index.js';
import {
  addCategory,
  changeCategory,
  retireCategory,
  useCategoryImpact,
} from '../../api/client.js';
import { printedDay, tierShort, tiers } from '../tiers.js';
import { useSaveChanges } from '../useSaveChanges.js';
import { StepDown, stepDownReady, usualStepDown } from './StepDown.js';

const fresh = (): CategoryInput => ({
  name: '',
  startTier: 'verbal',
  stepDown: usualStepDown(),
  statement: '',
  prior: '',
  plan: '',
});

/**
 * A category's name, starting tier, step down and wording, new or as it is; one in use can be
 * retired.
 */
export function CategoryDialog({
  category,
  close,
  saved,
  people,
}: {
  category?: ConductCategory;
  close: () => void;
  saved: (overview: ConductOverview) => void;
  people: ConductPerson[];
}) {
  const [start] = useState<CategoryInput>(() =>
    category
      ? {
          name: category.name,
          startTier: category.startTier,
          stepDown: category.stepDown,
          statement: category.statement,
          prior: category.prior,
          plan: category.plan,
          revision: category.revision,
        }
      : fresh(),
  );
  const [input, setInput] = useState(start);
  const set = (next: Partial<CategoryInput>) => setInput((now) => ({ ...now, ...next }));
  const save = useAction(
    async () => {
      saved(await (category ? changeCategory(category.id, input) : addCategory(input)));
      close();
    },
    { inline: true, success: 'Category saved' },
  );
  const retire = useAction(
    async () => {
      if (!category) return;
      saved(await retireCategory(category.id));
      close();
    },
    { inline: true, success: 'Category retired' },
  );
  const policyChanged = Boolean(
    category && JSON.stringify(input.stepDown) !== JSON.stringify(category.stepDown),
  );
  const impact = useCategoryImpact(
    category?.id ?? '',
    category?.revision ?? 1,
    input.stepDown,
    policyChanged && stepDownReady(input.stepDown),
  );
  const ready =
    [input.name, input.statement, input.prior, input.plan].every((text) => text.trim()) &&
    stepDownReady(input.stepDown) &&
    (!policyChanged || Boolean(impact.data));
  const { leave, asking } = useSaveChanges({
    what: 'This category',
    busy: save.busy || retire.busy,
    dirty: JSON.stringify(input) !== JSON.stringify(start),
    saveable: ready,
    save: () => save.run(),
  });
  return (
    <Modal title={category ? 'Edit category' : 'Add category'} onClose={() => leave(close)}>
      <form
        className="conduct-dialog"
        onSubmit={(event) => {
          event.preventDefault();
          if (ready) void save.run();
        }}
      >
        <div className="conduct-pair">
          <label className="conduct-field">
            <span className="conduct-label">Name</span>
            <input
              value={input.name}
              maxLength={60}
              onChange={(e) => set({ name: e.target.value })}
            />
          </label>
          <div className="conduct-field">
            <span className="conduct-label" id="conduct-starts">
              Starts at
            </span>
            <div className="conduct-tiers" role="radiogroup" aria-labelledby="conduct-starts">
              {tiers.map((tier) => (
                <button
                  key={tier}
                  type="button"
                  role="radio"
                  aria-checked={input.startTier === tier}
                  className={input.startTier === tier ? 'selected' : undefined}
                  onClick={() => set({ startTier: tier })}
                >
                  {tierShort[tier]}
                </button>
              ))}
            </div>
          </div>
        </div>
        <StepDown step={input.stepDown} change={(stepDown) => set({ stepDown })} />
        {policyChanged && (
          <DataState data={impact.data} error={impact.error} retry={impact.refresh}>
            {(changes) => (
              <section>
                <h3>Affected drivers ({changes.length})</h3>
                {changes.length > 0 && (
                  <div className="table-wrap">
                    <table className="conduct-table">
                      <thead>
                        <tr>
                          <th>Driver</th>
                          <th>Current</th>
                          <th>Proposed</th>
                          <th>Next step down</th>
                        </tr>
                      </thead>
                      <tbody>
                        {changes.map((change) => (
                          <tr key={change.person}>
                            <td>
                              {people.find((p) => p.code === change.person)?.name ?? change.person}
                            </td>
                            <td>{change.before ? tierShort[change.before.tier] : 'Clean'}</td>
                            <td>{change.after ? tierShort[change.after.tier] : 'Clean'}</td>
                            <td>
                              {change.after?.stepsDownOn
                                ? printedDay(change.after.stepsDownOn)
                                : '—'}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </section>
            )}
          </DataState>
        )}

        <label className="conduct-field">
          <span className="conduct-label">1. Statement of the problem</span>
          <textarea
            rows={3}
            maxLength={4000}
            value={input.statement}
            onChange={(e) => set({ statement: e.target.value })}
          />
        </label>
        <label className="conduct-field">
          <span className="conduct-label">2. Prior discussion or warnings</span>
          <textarea
            maxLength={4000}
            rows={3}
            value={input.prior}
            onChange={(e) => set({ prior: e.target.value })}
          />
        </label>
        <label className="conduct-field">
          <span className="conduct-label">3. Corrective action</span>
          <textarea
            maxLength={4000}
            rows={3}
            value={input.plan}
            onChange={(e) => set({ plan: e.target.value })}
          />
        </label>
        <ErrorBox message={save.error || retire.error} />
        <div className="form-actions">
          {category && (
            <button
              type="button"
              className="danger conduct-retire"
              disabled={retire.busy || save.busy}
              onClick={() => void retire.run()}
            >
              Retire
            </button>
          )}
          <button type="button" onClick={() => leave(close)}>
            Cancel
          </button>
          <button type="submit" className="primary" disabled={!ready || save.busy || retire.busy}>
            {save.busy ? 'Saving…' : 'Save'}
          </button>
        </div>
      </form>
      {asking}
    </Modal>
  );
}
