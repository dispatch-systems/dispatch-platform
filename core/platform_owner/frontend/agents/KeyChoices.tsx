import { useId } from 'react';
import { TriangleAlert } from 'lucide-react';
import type { AgentArea, AgentDsp, AgentKeyRequest, AgentSource } from '../../api/index.js';
import { areaGroups, areaHints, areaLabels, areaSources, areaWith, withArea } from './agents.js';

/** Where an app asking to connect reaches. */
type Scope = Pick<AgentKeyRequest, 'allDsps' | 'dsps'>;

/** Every DSP, those added later included, or the chosen ones, as an app is approved: a DSP
 * gets settings of its own only once the app is connected (`DspList`). */
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

function SwitchedOff({ id }: { id: string }) {
  return (
    <span className="agents-off" id={id}>
      <TriangleAlert size={13} aria-hidden="true" />
      Switched off here
    </span>
  );
}

/**
 * What a key or app may read, a switch each under the feature that collects it. Without `set`
 * the switches only show `areas`. `off` marks the features a DSP has switched off: a whole group,
 * or one kind of data when only its own feature is.
 */
export function ReadChoices({
  legend,
  hint,
  areas,
  set,
  off,
}: {
  legend: string;
  hint: string;
  areas: readonly AgentArea[];
  set?: (areas: AgentArea[]) => void;
  off?: readonly AgentSource[];
}) {
  const id = useId();
  const isOff = (area: AgentArea) => off?.includes(areaSources[area]) ?? false;
  return (
    <fieldset aria-describedby={`${id}-hint`}>
      <legend>{legend}</legend>
      <p className="agents-hint" id={`${id}-hint`}>
        {hint}
      </p>
      <div className={`agents-reads${set ? '' : ' following'}`}>
        {areaGroups.map((group) => {
          const groupId = `${id}-${group.label}`;
          const allOff = group.areas.every(isOff);
          return (
            <div
              key={group.label}
              className="agents-reads-group"
              role="group"
              aria-labelledby={groupId}
              aria-describedby={allOff ? `${groupId}-off` : undefined}
            >
              <header>
                <span id={groupId}>{group.label}</span>
                {allOff && <SwitchedOff id={`${groupId}-off`} />}
              </header>
              {group.areas.map((area) => {
                const about = areaHints[area] && `${id}-${area}-hint`;
                const offHere = !allOff && isOff(area) && `${id}-${area}-off`;
                const needs = areaWith[area];
                return (
                  <label key={area} className="agents-read">
                    <span>
                      <span id={`${id}-${area}`}>{areaLabels[area]}</span>
                      {about && <small id={about}>{areaHints[area]}</small>}
                      {offHere && <SwitchedOff id={offHere} />}
                    </span>
                    <input
                      type="checkbox"
                      role="switch"
                      aria-labelledby={`${id}-${area}`}
                      aria-describedby={[about, offHere].filter(Boolean).join(' ') || undefined}
                      checked={areas.includes(area)}
                      // A kind that comes with another needs that one on.
                      disabled={!set || (needs !== undefined && !areas.includes(needs))}
                      onChange={(event) => set?.(withArea(areas, area, event.target.checked))}
                    />
                  </label>
                );
              })}
            </div>
          );
        })}
      </div>
    </fieldset>
  );
}

// Every feature's data, with the first and the last feature the switches are listed under
// named as examples.
const examples = [...new Set([areaGroups[0]?.label, areaGroups.at(-1)?.label])].filter(
  (label) => label !== undefined,
);
const included = examples.length ? `, ${examples.join(' and ')} included,` : '';
/** What bypassing features does, wherever a key or app reads. */
export const bypassHint =
  `Reads every feature’s data${included} even where a DSP has switched the feature off. ` +
  'It only ever reads.';

/** One switch in a box, with what it does under it: bypassing features, which turns the box
 * amber while on, or a DSP following the key's settings. Without `set` it only shows `on`. */
export function SwitchBox({
  label,
  hint,
  on,
  set,
  tone,
}: {
  label: string;
  hint: string;
  on: boolean;
  set?: (on: boolean) => void;
  tone: 'bypass' | 'follow';
}) {
  const id = useId();
  return (
    <label className={`agents-box ${tone}${on ? ' on' : ''}`}>
      <span>
        <span id={`${id}-label`}>{label}</span>
        <small id={`${id}-hint`}>{hint}</small>
      </span>
      <input
        type="checkbox"
        role="switch"
        aria-labelledby={`${id}-label`}
        aria-describedby={`${id}-hint`}
        checked={on}
        disabled={!set}
        onChange={(event) => set?.(event.target.checked)}
      />
    </label>
  );
}
