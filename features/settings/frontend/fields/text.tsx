import type { SettingsValue } from '../../../../core/settings/api/index.js';
import type { Field, FieldKind, FieldProps } from './kind.js';

/** Whether words are ones the field takes: a locked one unchanged once set, an unset one left. */
function valid(field: Field<'text'>, value: SettingsValue, before: SettingsValue) {
  const text = typeof value === 'string' ? value.trim() : '';
  if (field.locked && before !== '') return text === before;
  if (!text) return before === '';
  return (
    text.length >= field.min &&
    text.length <= field.max &&
    (!field.code || /^[A-Za-z0-9]+$/.test(text))
  );
}

/** Words, or a code of letters and digits in capitals; a locked one can't change once set. */
function TextField({ field, id, value, before, change }: FieldProps<'text'>) {
  return (
    <input
      id={id}
      className={field.code ? 'settings-code' : undefined}
      minLength={field.min || undefined}
      maxLength={field.max}
      pattern={field.code ? '[A-Za-z0-9]*' : undefined}
      autoCapitalize={field.code ? 'characters' : undefined}
      spellCheck={field.code ? false : undefined}
      readOnly={field.locked && before !== ''}
      aria-invalid={!valid(field, value, before) || undefined}
      value={typeof value === 'string' ? value : ''}
      onChange={(event) =>
        change(field.code ? event.target.value.toUpperCase() : event.target.value)
      }
    />
  );
}

export const text: FieldKind<'text'> = {
  Control: TextField,
  valid,
  saved: (value) => (typeof value === 'string' ? value.trim() : value),
};
