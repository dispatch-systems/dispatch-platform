import type { FieldKind, FieldProps } from './kind.js';

function TimeField({ id, value, change }: FieldProps<'time'>) {
  return (
    <input
      id={id}
      type="time"
      required
      value={typeof value === 'string' ? value : ''}
      onChange={(event) => change(event.target.value)}
    />
  );
}

/** A time of day, `HH:MM`. */
export const time: FieldKind<'time'> = {
  Control: TimeField,
  valid: (_field, value) => typeof value === 'string' && /^\d{2}:\d{2}$/.test(value),
};
