import type { ConductStepDown, ConductStepKind, ConductUnit } from '../../api/index.js';

/** The longest wait a step down takes, as the server allows. */
const longest = 999;
const kinds: readonly (readonly [ConductStepKind | 'never', string])[] = [
  ['reduce', 'Reduce one tier'],
  ['reset', 'Full reset'],
  ['never', 'Never'],
];
const units: readonly (readonly [ConductUnit, string])[] = [
  ['days', 'Days'],
  ['weeks', 'Weeks'],
  ['months', 'Months'],
];

/** What a new category steps down by: a tier every six months. */
export const usualStepDown = (): ConductStepDown => ({ kind: 'reduce', after: 6, unit: 'months' });

/** Whether `step`'s wait is a whole number the server takes. */
export const stepDownReady = (step: ConductStepDown | null) =>
  !step || (Number.isInteger(step.after) && step.after >= 1 && step.after <= longest);

/** How a category's tiers step down: reduce one tier or fully reset, after how long, or never. */
export function StepDown({
  step,
  change,
}: {
  step: ConductStepDown | null;
  change: (step: ConductStepDown | null) => void;
}) {
  return (
    <div className="conduct-field">
      <span className="conduct-label" id="conduct-step">
        Reduce or reset
      </span>
      <div className="conduct-step" role="group" aria-labelledby="conduct-step">
        <select
          aria-label="Reduce or reset"
          value={step?.kind ?? 'never'}
          onChange={(e) => {
            const kind = e.target.value;
            change(
              kind === 'never'
                ? null
                : { ...(step ?? usualStepDown()), kind: kind as ConductStepKind },
            );
          }}
        >
          {kinds.map(([kind, label]) => (
            <option key={kind} value={kind}>
              {label}
            </option>
          ))}
        </select>
        {step && (
          <>
            <span>after</span>
            <input
              type="number"
              inputMode="numeric"
              min={1}
              max={longest}
              aria-label="After"
              value={step.after || ''}
              onChange={(e) => change({ ...step, after: Number(e.target.value) })}
            />
            <select
              aria-label="Wait in"
              value={step.unit}
              onChange={(e) => change({ ...step, unit: e.target.value as ConductUnit })}
            >
              {units.map(([unit, label]) => (
                <option key={unit} value={unit}>
                  {label}
                </option>
              ))}
            </select>
          </>
        )}
      </div>
    </div>
  );
}

/** The policy as a category row names it. */
export function stepDownRule(step: ConductStepDown | null) {
  if (!step) return 'Never';
  const unit = step.after === 1 ? step.unit.slice(0, -1) : step.unit;
  return `${step.kind === 'reset' ? 'Full reset' : 'Reduce one tier'} · ${step.after} ${unit}`;
}
