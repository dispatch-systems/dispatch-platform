import type { SettingsField, SettingsValue } from '../../../../core/settings/api/index.js';
import { choice } from './choice.js';
import { days } from './days.js';
import type { FieldKind, FieldProps } from './kind.js';
import { many } from './many.js';
import { number } from './number.js';
import { pick } from './pick.js';
import { text } from './text.js';
import { time } from './time.js';
import { timezone } from './timezone.js';
import { toggle } from './toggle.js';

// Every kind of field the page draws, each in its own module. A new kind is its module and its
// entry here; the type holds every kind the server sends to one.
const kinds: { [K in SettingsField['kind']]: FieldKind<K> } = {
  toggle,
  choice,
  pick,
  many,
  number,
  text,
  days,
  time,
  timezone,
};
const kindOf = (field: SettingsField) =>
  kinds[field.kind] as unknown as FieldKind<SettingsField['kind']>;

/** Whether a field is saved with its section: one that saves itself is not. */
export const held = (field: SettingsField) => !kindOf(field).apart;
/** The value to save for what a field holds. */
export const saved = (field: SettingsField, value: SettingsValue) =>
  kindOf(field).saved?.(value) ?? value;
/** Whether what a field holds may be saved, given the value it had. */
export const valid = (field: SettingsField, value: SettingsValue, before: SettingsValue) =>
  kindOf(field).valid?.(field, value, before) ?? true;

/** A field: its label beside its control, or the control alone where it names itself. */
export function FieldRow(props: FieldProps<SettingsField['kind']>) {
  const { field, id } = props;
  const { Control, labelled } = kindOf(field);
  if (labelled?.(field)) return <Control {...props} />;
  return (
    <div className="settings-field">
      <label htmlFor={id}>{field.label}</label>
      <Control {...props} />
    </div>
  );
}
