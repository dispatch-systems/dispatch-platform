import type { NameOrder, PaycomSettings } from '../api/index.js';
export const paycomColumns = [
  ['inDay', 'Clock in'],
  ['outLunch', 'Lunch out'],
  ['inLunch', 'Lunch in'],
  ['outDay', 'Clock out'],
  ['totalHours', 'Hours'],
  ['condition', 'Punch status'],
] as const;
export type PaycomColumn = (typeof paycomColumns)[number][0];
/** What the page shows by until the DSP's settings are read: as a DSP that saved none. */
export const paycomDefaults: PaycomSettings = {
  nameOrder: 'first_last',
  driverDepartments: null,
  late: { time: '10:01', departments: [] },
};
export function employeeName(name: string, order: NameOrder) {
  const parts = name.trim().split(/\s+/);
  return order === 'last_first' && parts.length > 1
    ? `${parts.at(-1)}, ${parts.slice(0, -1).join(' ')}`
    : name;
}
