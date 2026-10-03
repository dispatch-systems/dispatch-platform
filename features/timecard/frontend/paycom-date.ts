import { localDate, shiftDate } from './meal-breaks.js';

export function validPaycomDate(value: string, today: string) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value < '2000-01-01' || value > today) return false;
  try {
    return shiftDate(value, 0) === value;
  } catch {
    return false;
  }
}

export const paycomDateKey = (dspId: string) => `dispatch:paycom-date:${dspId}`;
export function selectedPaycomDate(dspId: string, timezone: string) {
  const today = localDate(timezone);
  try {
    const saved = sessionStorage.getItem(paycomDateKey(dspId));
    if (saved && validPaycomDate(saved, today)) return saved;
  } catch {
    // Navigation still works with restricted browser storage.
  }
  return today;
}
