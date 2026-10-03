import type { Ref } from 'react';
import { ChevronRight } from 'lucide-react';
import type {
  AgentKeyDsp,
  AgentKeyRequest,
  AgentReads,
} from '../../../../shared/contracts/platform-owner.js';
import { bypassHere, switchedOffText } from './agents.js';
import { ReadChoices, SwitchBox } from './KeyChoices.js';

type Props = {
  form: AgentKeyRequest;
  set: (change: Partial<AgentKeyRequest>) => void;
  /** What the sheet edits: a key, or a connected app. */
  noun: 'key' | 'app';
};

/**
 * Every DSP, those added later included, or the chosen ones; each reached DSP says whether it
 * follows the key's settings or has its own, and opens them. `editId` names each DSP's button,
 * so leaving a DSP's settings returns to it.
 */
export function DspList({
  dsps,
  form,
  set,
  noun,
  edit,
  editId,
}: Props & {
  dsps: AgentKeyDsp[];
  edit: (dsp: string) => void;
  editId: (dsp: string) => string;
}) {
  const toggle = (id: string, on: boolean) =>
    set({ dsps: on ? [...form.dsps, id] : form.dsps.filter((dsp) => dsp !== id) });
  return (
    <fieldset>
      <legend>DSPs</legend>
      <div className="agents-choices two">
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
            <small>Only the DSPs you pick</small>
          </span>
        </label>
      </div>
      {dsps.length > 0 && (
        <ul className="agents-dsp-list">
          {dsps.map((dsp) => {
            const reached = form.allDsps || form.dsps.includes(dsp.id);
            const own = form.dspReads.find((reads) => reads.dsp === dsp.id);
            return (
              <li key={dsp.id} className={reached ? undefined : 'out'}>
                {form.allDsps ? (
                  <span className="agents-dsp-name">{dsp.name}</span>
                ) : (
                  <label className="agents-dsp-name">
                    <input
                      type="checkbox"
                      checked={reached}
                      onChange={(event) => toggle(dsp.id, event.target.checked)}
                    />
                    {dsp.name}
                  </label>
                )}
                {reached && (
                  <>
                    <span className={`agents-dsp-state${own ? ' own' : ''}`}>
                      {own
                        ? `Own settings${own.bypass ? ' · Bypass on' : ''}`
                        : `${noun === 'app' ? 'App' : 'Key'} settings`}
                    </span>
                    <button
                      type="button"
                      id={editId(dsp.id)}
                      aria-label={`Edit ${dsp.name}`}
                      onClick={() => edit(dsp.id)}
                    >
                      Edit
                      <ChevronRight size={14} aria-hidden="true" />
                    </button>
                  </>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </fieldset>
  );
}

/**
 * What a key reads at one DSP: the key's own settings, shown but not changeable here, or the
 * DSP's own, which start as a copy of the key's and then stay as they are when the key's change.
 * Done goes back; nothing is saved until the sheet is.
 */
export function DspView({
  ref,
  dsp,
  form,
  set,
  noun,
  done,
}: Props & { ref: Ref<HTMLFormElement>; dsp: AgentKeyDsp; done: () => void }) {
  const own = form.dspReads.find((reads) => reads.dsp === dsp.id);
  const others = form.dspReads.filter((reads) => reads.dsp !== dsp.id);
  const reads = own ?? form.reads;
  // Changing what the DSP reads makes it its own, from what it reads now.
  const change = (next: Partial<AgentReads>) =>
    set({
      dspReads: [...others, { dsp: dsp.id, areas: reads.areas, bypass: reads.bypass, ...next }],
    });
  const name = form.name.trim();
  const who = name || `this ${noun}`;
  const settings = `${name ? `${name}’s` : `the ${noun}’s`} settings`;
  return (
    <form
      ref={ref}
      className="agents-form agents-sheet"
      onSubmit={(event) => {
        event.preventDefault();
        done();
      }}
    >
      <SwitchBox
        tone="follow"
        label={`Use ${noun} settings`}
        hint={
          own
            ? `Off: ${dsp.name} has its own settings. Changing the ${noun}’s settings leaves ` +
              'these as they are.'
            : `${dsp.name} follows ${settings}. Turn this off to give it its own.`
        }
        on={!own}
        set={(follow) => (follow ? set({ dspReads: others }) : change({}))}
      />
      <ReadChoices
        legend={`What ${who} can read here`}
        hint={switchedOffText(dsp)}
        areas={reads.areas}
        set={own && ((areas) => change({ areas }))}
        off={dsp.switchedOff}
      />
      <SwitchBox
        tone="bypass"
        label="Bypass features"
        hint={
          own
            ? bypassHere(name || `This ${noun}`, dsp)
            : `Follows ${settings}: ${form.reads.bypass ? 'on' : 'off'}.`
        }
        on={reads.bypass}
        set={own && ((bypass) => change({ bypass }))}
      />
      <div className="form-actions">
        <button className="primary">Done</button>
      </div>
    </form>
  );
}
