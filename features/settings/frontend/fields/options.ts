import type { SettingsOption, SettingsValue } from '../../../../core/settings/api/index.js';

// The options a field chooses among, as its declaration or its feature lists them when Settings
// is read. A value saved before the list dropped it, as a department no longer on the roster,
// stays listed, by the value itself, until the member chooses it away and saves: an edit never
// loses it unasked.

/** The field's options, then each of the values it holds or held that they lack. */
export function listed(
  options: readonly SettingsOption[],
  held: readonly SettingsValue[],
): readonly SettingsOption[] {
  const lacked = held.filter(
    (value): value is string =>
      typeof value === 'string' && !options.some((option) => option.value === value),
  );
  return [...options, ...[...new Set(lacked)].map((value) => ({ value, label: value }))];
}

/** What a field of several holds once `value` is checked or not, in the order `shown` lists. */
export const checking = (
  shown: readonly SettingsOption[],
  picked: readonly string[],
  value: string,
  checked: boolean,
) =>
  shown
    .map((option) => option.value)
    .filter((each) => (each === value ? checked : picked.includes(each)));
