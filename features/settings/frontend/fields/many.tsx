import { ChevronDown, ChevronUp } from 'lucide-react';
import type { SettingsValue } from '../../../../core/settings/api/index.js';
import type { FieldKind, FieldProps } from './kind.js';

const chosen = (value: SettingsValue) => (Array.isArray(value) ? value : []);

/** Each option checked or not, kept in the options' order. */
function Checks({ field, id, value, change }: FieldProps<'many'>) {
  const picked = chosen(value);
  return (
    <div className="settings-checks">
      {field.options.map((option) => (
        <label key={option.value}>
          <input
            type="checkbox"
            name={id}
            checked={picked.includes(option.value)}
            onChange={(event) =>
              change(
                field.options
                  .map((each) => each.value)
                  .filter((each) =>
                    each === option.value ? event.target.checked : picked.includes(each),
                  ),
              )
            }
          />
          {option.label}
        </label>
      ))}
    </div>
  );
}

/** The checked options first, in the member's order, each moved up or down; the rest after. */
function Order({ field, id, value, change }: FieldProps<'many'>) {
  const picked = chosen(value);
  const label = (each: string) =>
    field.options.find((option) => option.value === each)?.label ?? each;
  const move = (from: number, to: number) => {
    const next = [...picked];
    next.splice(to, 0, ...next.splice(from, 1));
    change(next);
  };
  return (
    <ol className="settings-order">
      {picked.map((each, index) => (
        <li key={each}>
          <label>
            <input
              type="checkbox"
              name={id}
              checked
              onChange={() => change(picked.filter((other) => other !== each))}
            />
            {label(each)}
          </label>
          <button
            type="button"
            className="icon-button"
            aria-label={`Move ${label(each)} up`}
            disabled={index === 0}
            onClick={() => move(index, index - 1)}
          >
            <ChevronUp size={16} aria-hidden="true" />
          </button>
          <button
            type="button"
            className="icon-button"
            aria-label={`Move ${label(each)} down`}
            disabled={index === picked.length - 1}
            onClick={() => move(index, index + 1)}
          >
            <ChevronDown size={16} aria-hidden="true" />
          </button>
        </li>
      ))}
      {field.options
        .filter((option) => !picked.includes(option.value))
        .map((option) => (
          <li key={option.value}>
            <label>
              <input
                type="checkbox"
                name={id}
                checked={false}
                onChange={() => change([...picked, option.value])}
              />
              {option.label}
            </label>
          </li>
        ))}
    </ol>
  );
}

function Many(props: FieldProps<'many'>) {
  const { field, id } = props;
  return (
    <div className="settings-field settings-wide" role="group" aria-labelledby={`${id}-label`}>
      <span id={`${id}-label`}>{field.label}</span>
      {field.ordered ? <Order {...props} /> : <Checks {...props} />}
    </div>
  );
}

/** Several of the options, in their order or the member's. */
export const many: FieldKind<'many'> = { Control: Many, labelled: () => true };
