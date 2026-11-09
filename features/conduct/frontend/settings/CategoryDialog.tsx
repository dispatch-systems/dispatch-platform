import { useState } from 'react';
import { ErrorBox, Modal } from '../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { CategoryInput, ConductCategory, ConductOverview } from '../../api/index.js';
import { addCategory, changeCategory, retireCategory } from '../../api/client.js';
import { tierShort, tiers } from '../tiers.js';
import { SaveChanges } from '../SaveChanges.js';
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
}: {
  category?: ConductCategory;
  close: () => void;
  saved: (overview: ConductOverview) => void;
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
  const ready =
    [input.name, input.statement, input.prior, input.plan].every((text) => text.trim()) &&
    stepDownReady(input.stepDown);
  const [confirming, setConfirming] = useState(false);
  // Changes leave only through Save or Discard.
  const leave = () =>
    JSON.stringify(input) !== JSON.stringify(start) ? setConfirming(true) : close();
  return (
    <Modal title={category ? 'Edit category' : 'Add category'} onClose={leave}>
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
        <label className="conduct-field">
          <span className="conduct-label">1. Statement of the problem</span>
          <textarea
            rows={3}
            value={input.statement}
            onChange={(e) => set({ statement: e.target.value })}
          />
        </label>
        <label className="conduct-field">
          <span className="conduct-label">2. Prior discussion or warnings</span>
          <textarea rows={3} value={input.prior} onChange={(e) => set({ prior: e.target.value })} />
        </label>
        <label className="conduct-field">
          <span className="conduct-label">3. Corrective action</span>
          <textarea rows={3} value={input.plan} onChange={(e) => set({ plan: e.target.value })} />
        </label>
        <ErrorBox message={save.error || retire.error} />
        <div className="form-actions">
          {category && (
            <button
              type="button"
              className="danger conduct-retire"
              disabled={retire.busy}
              onClick={() => void retire.run()}
            >
              Retire
            </button>
          )}
          <button type="button" onClick={leave}>
            Cancel
          </button>
          <button type="submit" className="primary" disabled={!ready || save.busy}>
            {save.busy ? 'Saving…' : 'Save'}
          </button>
        </div>
      </form>
      {confirming && (
        <SaveChanges
          what="This category"
          saveable={ready}
          busy={save.busy}
          save={async () => {
            if (!(await save.run())) setConfirming(false);
          }}
          discard={close}
        />
      )}
    </Modal>
  );
}
