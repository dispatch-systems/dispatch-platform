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
  const toggle = (id: string, on: boolean) =>
    set({ dsps: on ? [...form.dsps, id] : form.dsps.filter((dsp) => dsp !== id) });
  return (
    <fieldset>
      <legend>DSPs</legend>
      <div className="agents-choices">
        <label className="agents-choice">
          <input
            type="radio"
            name="dsps"
            checked={form.allDsps}
            onChange={() => set({ allDsps: true, dsps: [] })}
          />
          <span>
            <strong>All DSPs</strong>
            <small>Including DSPs added later</small>
          </span>
        </label>
        <label className="agents-choice">
          <input
            type="radio"
            name="dsps"
            checked={!form.allDsps}
            onChange={() => set({ allDsps: false })}
          />
          <span>
            <strong>Choose DSPs</strong>
            {!form.allDsps && (
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
          </span>
        </label>
      </div>
    </fieldset>
  );
}
