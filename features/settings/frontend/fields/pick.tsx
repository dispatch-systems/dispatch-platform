import { OptionSelect } from './choice.js';
import type { FieldKind, FieldProps } from './kind.js';

function Pick({ field, id, value, change }: FieldProps<'pick'>) {
  return (
    <OptionSelect id={id} options={field.options} none={field.none} value={value} change={change} />
  );
}

/** One of the options its feature lists as Settings is read, or none where it may be. */
export const pick: FieldKind<'pick'> = {
  Control: Pick,
  valid: (field, value) => value !== null || field.none !== null,
};
