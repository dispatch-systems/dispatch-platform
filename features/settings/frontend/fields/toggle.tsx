import type { FieldKind, FieldProps } from './kind.js';

/** On or off, as a switch beside its label. */
function Toggle({ field, id, value, change }: FieldProps<'toggle'>) {
  return (
    <label className="settings-field settings-toggle" htmlFor={id}>
      <span>{field.label}</span>
      <input
        id={id}
        type="checkbox"
        role="switch"
        checked={value === true}
        onChange={(event) => change(event.target.checked)}
      />
    </label>
  );
}

export const toggle: FieldKind<'toggle'> = { Control: Toggle, labelled: () => true };
