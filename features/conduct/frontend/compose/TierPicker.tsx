import type { ConductFinalKind, ConductTier, ConductWriteupRow } from '../../api/index.js';
import { TierBadge } from '../parts.js';
import { printedDay, tierShort, tiers } from '../tiers.js';

/** What a final writeup comes with, as the picker offers it. */
export type FinalChoice = 'none' | ConductFinalKind;
const finals: readonly (readonly [FinalChoice, string])[] = [
  ['none', 'Final Written Warning'],
  ['suspension', 'With a suspension'],
  ['investigation', 'Pending further investigation'],
];

/**
 * The tier, the suggested one marked, with the earlier writeups that still count beneath; for
 * a final one, what it comes with, and how long a suspension is.
 */
export function TierPicker({
  tier,
  suggested,
  earlier,
  atFinal,
  final,
  suspension,
  change,
}: {
  tier: ConductTier;
  suggested?: ConductTier;
  earlier: readonly ConductWriteupRow[];
  atFinal: boolean;
  final: FinalChoice;
  suspension: string;
  change: (next: { tier?: ConductTier; final?: FinalChoice; suspension?: string }) => void;
}) {
  return (
    <div className="conduct-field">
      <span className="conduct-label" id="conduct-tier">
        Tier
      </span>
      <div className="conduct-tiers" role="radiogroup" aria-labelledby="conduct-tier">
        {tiers.map((t) => (
          <button
            key={t}
            type="button"
            role="radio"
            aria-checked={tier === t}
            className={tier === t ? 'selected' : undefined}
            onClick={() => change({ tier: t })}
          >
            {tierShort[t]}
            {suggested === t && <small>Suggested</small>}
          </button>
        ))}
      </div>
      {earlier.length > 0 && (
        <div className="conduct-history">
          {[...earlier].reverse().map((w) => (
            <span key={w.id}>
              <TierBadge tier={w.tier} />
              <span className="muted">{printedDay(w.writtenOn)}</span>
            </span>
          ))}
          {atFinal && <span className="conduct-tier conduct-tier-final">At Final</span>}
        </div>
      )}
      {tier === 'final' && (
        <div className="conduct-finals" role="radiogroup" aria-label="Final">
          {finals.map(([kind, label]) => (
            <label key={kind}>
              <input
                type="radio"
                name="conduct-final"
                checked={final === kind}
                onChange={() => change({ final: kind })}
              />
              {label}
              {kind === 'suspension' && final === 'suspension' && (
                <input
                  className="conduct-suspension"
                  aria-label="Suspension length"
                  placeholder="1 day"
                  maxLength={40}
                  value={suspension}
                  onChange={(event) => change({ suspension: event.target.value })}
                />
              )}
            </label>
          ))}
        </div>
      )}
    </div>
  );
}
