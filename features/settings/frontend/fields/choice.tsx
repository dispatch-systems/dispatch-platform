import type { SettingsOption, SettingsValue } from '../../../../core/settings/api/index.js';
import type { FieldKind, FieldProps } from './kind.js';

/** One of the options from a list, a saved one the list no longer has kept in it. */
export function OptionSelect({
  id,
  options,
  none,
  value,
  change,
}: {
  id: string;
  options: readonly SettingsOption[];
  /** What no option is called, when none may be chosen. */
  none: string | null;
  value: SettingsValue;
  change: (value: SettingsValue) => void;
}) {
  const listed = [
    ...options,
    ...(typeof value === 'string' && !options.some((option) => option.value === value)
      ? [{ value, label: value }]
      : []),
  ];
  return (
    <select
      id={id}
      value={value === null ? '' : String(value)}
      onChange={(event) => change(event.target.value === '' ? null : event.target.value)}
    >
      {(none !== null || value === null) && (
        <option value="" disabled={none === null}>
          {none ?? ''}
        </option>
      )}
      {listed.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
  );
}

// A few options side by side; more, in a list.
const few = (options: readonly SettingsOption[]) => options.length <= 3;

function Choice({ field, id, value, change }: FieldProps<'choice'>) {
  if (!few(field.options))
    return (
      <OptionSelect id={id} options={field.options} none={null} value={value} change={change} />
    );
  return (
    <div className="settings-field" role="radiogroup" aria-labelledby={`${id}-label`}>
      <span id={`${id}-label`}>{field.label}</span>
      <div className="settings-segments">
        {field.options.map((option) => (
          <label key={option.value}>
            <input
              type="radio"
              name={id}
              value={option.value}
              checked={value === option.value}
              onChange={() => change(option.value)}
            />
            {option.label}
          </label>
        ))}
      </div>
    </div>
  );
}

/** One of fixed options. */
export const choice: FieldKind<'choice'> = {
  Control: Choice,
  labelled: (field) => few(field.options),
};
