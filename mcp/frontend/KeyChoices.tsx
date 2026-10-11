import { useId } from 'react';
import type { AgentDsp, AgentKeyRequest } from '../api/index.js';

/** Where a key or app reaches. */
type Scope = Pick<AgentKeyRequest, 'allDsps' | 'dsps'>;

/** Every DSP, those added later included, or the chosen ones. */
export function DspReach({
  dsps,
  form,
  set,
}: {
  dsps: AgentDsp[];
  form: Scope;
  set: (change: Partial<Scope>) => void;
}) {
  const id = useId();
  const toggle = (dsp: string, on: boolean) =>
    set({ dsps: on ? [...form.dsps, dsp] : form.dsps.filter((chosen) => chosen !== dsp) });
  return (
    <fieldset>
      <legend>DSPs</legend>
      <div className="agents-levels wide">
        <label className="agents-level">
          <input
            type="radio"
            name={`${id}-dsps`}
            checked={form.allDsps}
            onChange={() => set({ allDsps: true, dsps: [] })}
          />
          <span>All DSPs</span>
        </label>
        <label className="agents-level">
          <input
            type="radio"
            name={`${id}-dsps`}
            checked={!form.allDsps}
            onChange={() => set({ allDsps: false })}
          />
          <span>Choose DSPs</span>
        </label>
      </div>
      {form.allDsps ? (
        <p className="agents-hint agents-under">Includes DSPs added later.</p>
      ) : (
        <span className="agents-dsps">
          {dsps.map((dsp) => (
            <label key={dsp.id}>
              <input
                type="checkbox"
                checked={form.dsps.includes(dsp.id)}
                onChange={(event) => toggle(dsp.id, event.target.checked)}
              />
              {dsp.name}
            </label>
          ))}
        </span>
      )}
    </fieldset>
  );
}
