import { useId, type ReactNode } from 'react';

// The controls Message display's choices are made with, drawn as Settings draws its own fields:
// a row each, its label beside or over what it chooses.

/** A few options side by side, one chosen. */
export function Segments<T extends string>({
  label,
  value,
  options,
  change,
}: {
  label: string;
  value: T;
  options: readonly (readonly [T, string])[];
  change: (value: T) => void;
}) {
  const id = useId();
  return (
    <div className="settings-field">
      <span id={id}>{label}</span>
      <Choice labelledBy={id} value={value} options={options} change={change} />
    </div>
  );
}
/** Segments without a row of their own, as the preview's. */
export function Choice<T extends string>({
  labelledBy,
  label,
  value,
  options,
  change,
}: {
  labelledBy?: string;
  label?: string;
  value: T;
  options: readonly (readonly [T, string])[];
  change: (value: T) => void;
}) {
  const name = useId();
  return (
    <div
      className="settings-segments"
      role="radiogroup"
      aria-labelledby={labelledBy}
      aria-label={label}
    >
      {options.map(([option, text]) => (
        <label key={option}>
          <input
            type="radio"
            name={name}
            checked={value === option}
            onChange={() => change(option)}
          />
          {text}
        </label>
      ))}
    </div>
  );
}

/** Options each shown by a picture of what it does, one chosen. */
export function Tiles<T extends string>({
  label,
  value,
  options,
  change,
}: {
  label: string;
  value: T;
  options: readonly (readonly [T, string, ReactNode])[];
  change: (value: T) => void;
}) {
  const id = useId();
  const name = useId();
  return (
    <div className="settings-field settings-wide">
      <span id={id}>{label}</span>
      <div className="display-tiles" role="radiogroup" aria-labelledby={id}>
        {options.map(([option, text, picture]) => (
          <label key={option} className="display-tile">
            <input
              type="radio"
              name={name}
              checked={value === option}
              onChange={() => change(option)}
            />
            {picture}
            <span>{text}</span>
          </label>
        ))}
      </div>
    </div>
  );
}

/** A number on a line from `min` to `max`, shown with its unit; `ends` marks the line's ends. */
export function Slider({
  label,
  value,
  min,
  max,
  step = 1,
  unit,
  ends,
  change,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  unit: string;
  ends?: [ReactNode, ReactNode];
  change: (value: number) => void;
}) {
  const id = useId();
  return (
    <div className="settings-field settings-wide display-slider">
      <div className="display-slider-head">
        <label htmlFor={id}>{label}</label>
        <output htmlFor={id}>
          {value}
          {unit === '%' ? '%' : ` ${unit}`}
        </output>
      </div>
      <div className="display-slider-line">
        {ends && <span aria-hidden="true">{ends[0]}</span>}
        <input
          id={id}
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(event) => change(Number(event.target.value))}
        />
        {ends && <span aria-hidden="true">{ends[1]}</span>}
      </div>
    </div>
  );
}

/** On or off. */
export function Switch({
  label,
  on,
  change,
}: {
  label: string;
  on: boolean;
  change: (on: boolean) => void;
}) {
  return (
    <label className="settings-field settings-toggle">
      <span>{label}</span>
      <input
        type="checkbox"
        role="switch"
        checked={on}
        onChange={(event) => change(event.target.checked)}
      />
    </label>
  );
}

/** Colours to choose one of, each as the page has it: your team's or the employees'. */
export function Swatches<T extends string>({
  label,
  of,
  value,
  options,
  change,
}: {
  label: string;
  of: 'team' | 'employee';
  value: T;
  options: readonly (readonly [T, string])[];
  change: (value: T) => void;
}) {
  const id = useId();
  const name = useId();
  return (
    <div className="settings-field settings-wide">
      <span id={id}>{label}</span>
      <div className="swatches" role="radiogroup" aria-labelledby={id}>
        {options.map(([color, text]) => (
          <label key={color} className="swatch-choice">
            <input
              type="radio"
              name={name}
              aria-label={text}
              checked={value === color}
              onChange={() => change(color)}
            />
            <span className={`swatch ${of}`} {...{ [`data-${of}-color`]: color }} />
          </label>
        ))}
      </div>
    </div>
  );
}

/** A picture of a layout: the list, the messages and, for the profile's, its panel. */
export function Sketch({
  kind,
}: {
  kind: 'full' | 'centred' | 'bubbles' | 'plain' | 'beside' | 'over';
}) {
  return (
    <span className={`sketch sketch-${kind}`} aria-hidden="true">
      <span className="sketch-list">
        <i />
        <i />
        <i />
        <i />
      </span>
      <span className="sketch-pane">
        <b className="in" />
        <b className="out" />
        <b className="in short" />
        <b className="out short" />
      </span>
      {(kind === 'beside' || kind === 'over') && (
        <span className="sketch-profile">
          <u />
          <i />
          <i />
          <i />
        </span>
      )}
    </span>
  );
}
