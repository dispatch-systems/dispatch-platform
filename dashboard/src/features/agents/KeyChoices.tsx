import { useId } from 'react';
import type { AgentDsp, AgentKeyRequest } from '../../../../shared/contracts/index.js';

/** What a key or a connected app reaches and answers with, as both of their forms offer it. */
type Scope = Pick<AgentKeyRequest, 'allDsps' | 'dsps' | 'tools' | 'locations'>;
type Props = { form: Scope; set: (change: Partial<Scope>) => void };

/** Every DSP, those added later included, or the chosen ones. */
export function DspReach({ dsps, form, set }: Props & { dsps: AgentDsp[] }) {
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

/** Full or Essential tools. */
export function ToolChoice({ form, set }: Props) {
  const id = useId();
  return (
    <div className="agents-row">
      <span id={id}>Tools</span>
      <div className="agents-segmented" role="group" aria-labelledby={id}>
        {(['full', 'essential'] as const).map((tools) => (
          <button
            key={tools}
            type="button"
            aria-pressed={form.tools === tools}
            onClick={() => set({ tools })}
          >
            {tools === 'full' ? 'Full' : 'Essential'}
          </button>
        ))}
      </div>
    </div>
  );
}

/** Whether answers carry delivery addresses and GPS. */
export function LocationsSwitch({ form, set }: Props) {
  return (
    <label className="agents-row">
      Delivery addresses and GPS
      <input
        type="checkbox"
        role="switch"
        checked={form.locations}
        onChange={(event) => set({ locations: event.target.checked })}
      />
    </label>
  );
}
