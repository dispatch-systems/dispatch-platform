import { useState } from 'react';
import type { SettingsValue } from '../../../../core/settings/api/index.js';
import type { Field, FieldKind, FieldProps } from './kind.js';

const CUSTOM = 'custom';
const NONE = 'none';
const named = (words: string) => words.charAt(0).toUpperCase() + words.slice(1);

/** Whether a whole number is one the field takes: in its range, or none where it may be. */
function valid(field: Field<'number'>, value: SettingsValue) {
  if (value === null) return field.none !== null;
  const number = Number(value);
  return (
    (typeof value !== 'string' || /^\d+$/.test(value.trim())) &&
    Number.isInteger(number) &&
    number >= field.min &&
    number <= field.max
  );
}

/**
 * A whole number in its unit, typed; with presets or a value for none, chosen from a list
 * where Custom shows it to type.
 */
function NumberField({ field, id, value, before, change }: FieldProps<'number'>) {
  const listed = field.presets.length > 0 || field.none !== null;
  const preset =
    value === null
      ? NONE
      : (field.presets.find((each) => String(each.value) === String(value).trim())?.value ??
        CUSTOM);
  // Custom shows the number to type once chosen, and for one no preset is.
  const [chose, setChose] = useState(false);
  const custom = chose || preset === CUSTOM;
  const typed = (
    <span className="settings-number">
      <input
        id={listed ? undefined : id}
        type="number"
        inputMode="numeric"
        min={field.min}
        max={field.max}
        step={1}
        required
        aria-label={listed ? named(field.unit ?? field.label) : undefined}
        aria-invalid={!valid(field, value) || undefined}
        value={value === null ? '' : String(value)}
        onChange={(event) => change(event.target.value)}
      />
      {field.unit && <span aria-hidden={listed || undefined}>{field.unit}</span>}
    </span>
  );
  if (!listed) return typed;
  return (
    <span className="settings-number-choice">
      <select
        id={id}
        value={custom ? CUSTOM : String(preset)}
        onChange={(event) => {
          const next = event.target.value;
          setChose(next === CUSTOM);
          if (next === NONE) change(null);
          else if (next !== CUSTOM) change(Number(next));
          else if (value === null) change(typeof before === 'number' ? before : field.min);
        }}
      >
        {field.none !== null && <option value={NONE}>{field.none}</option>}
        {field.presets.map((each) => (
          <option key={each.value} value={String(each.value)}>
            {each.label}
          </option>
        ))}
        <option value={CUSTOM}>Custom</option>
      </select>
      {custom && typed}
    </span>
  );
}

/** A whole number in a range, with its unit, presets and a value for none. */
export const number: FieldKind<'number'> = {
  Control: NumberField,
  valid,
  saved: (value) => (typeof value === 'string' ? Number(value) : value),
};
