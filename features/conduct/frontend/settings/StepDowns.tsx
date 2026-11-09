import type { ConductStepDown, ConductTier, ConductUnit } from '../../api/index.js';
import { TierBadge } from '../parts.js';
import { tierShort, tiers } from '../tiers.js';

/** The longest wait a step down takes, as the server allows. */
const longest = 999;
const units: readonly (readonly [ConductUnit, string])[] = [
  ['days', 'Days'],
  ['weeks', 'Weeks'],
  ['months', 'Months'],
];

/** The tiers `tier` can step down to, highest first; none is a clean record. */
const lower = (tier: ConductTier) => tiers.slice(0, tiers.indexOf(tier)).reverse();

/** A tier's step down until someone changes it: one tier, after six months. */
const usual = (tier: ConductTier): ConductStepDown => ({
  tier,
  after: 6,
  unit: 'months',
  to: lower(tier)[0] ?? null,
});

/** What a new category steps down by. */
export const usualStepDowns = (): ConductStepDown[] => [...tiers].reverse().map(usual);

/** Whether every wait is a whole number the server takes. */
export const stepDownsReady = (steps: readonly ConductStepDown[]) =>
  steps.every((step) => Number.isInteger(step.after) && step.after >= 1 && step.after <= longest);

/** Each tier's step down, highest first: how long until, and whether it reduces to a lower
 * tier or fully resets. */
export function StepDowns({
  steps,
  change,
}: {
  steps: readonly ConductStepDown[];
  change: (steps: ConductStepDown[]) => void;
}) {
  const set = (tier: ConductTier, step: ConductStepDown | undefined) =>
    change([...steps.filter((s) => s.tier !== tier), ...(step ? [step] : [])]);
  return (
    <div className="conduct-field">
      <span className="conduct-label" id="conduct-steps">
        Reduce or reset
      </span>
      <table className="conduct-steps" aria-labelledby="conduct-steps">
        <thead>
          <tr>
            <th>Tier</th>
            <th>After</th>
            <th>Then</th>
          </tr>
        </thead>
        <tbody>
          {[...tiers].reverse().map((tier) => {
            const step = steps.find((s) => s.tier === tier);
            const name = tierShort[tier];
            return (
              <tr key={tier}>
                <td>
                  <TierBadge tier={tier} />
                </td>
                <td>
                  <div className="conduct-step-after">
                    {step && (
                      <input
                        type="number"
                        inputMode="numeric"
                        min={1}
                        max={longest}
                        aria-label={`${name} steps down after`}
                        value={step.after || ''}
                        onChange={(e) => set(tier, { ...step, after: Number(e.target.value) })}
                      />
                    )}
                    <select
                      aria-label={`${name} wait in`}
                      value={step?.unit ?? 'never'}
                      onChange={(e) => {
                        const unit = e.target.value;
                        if (unit === 'never') set(tier, undefined);
                        else set(tier, { ...(step ?? usual(tier)), unit: unit as ConductUnit });
                      }}
                    >
                      {units.map(([unit, label]) => (
                        <option key={unit} value={unit}>
                          {label}
                        </option>
                      ))}
                      <option value="never">Never</option>
                    </select>
                  </div>
                </td>
                <td>
                  {step &&
                    (lower(tier).length === 0 ? (
                      'Full reset'
                    ) : (
                      <select
                        aria-label={`${name} then`}
                        value={step.to ?? 'reset'}
                        onChange={(e) => {
                          const to = e.target.value;
                          set(tier, { ...step, to: to === 'reset' ? null : (to as ConductTier) });
                        }}
                      >
                        {lower(tier).map((to) => (
                          <option key={to} value={to}>
                            Reduce to {tierShort[to]}
                          </option>
                        ))}
                        <option value="reset">Full reset</option>
                      </select>
                    ))}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
