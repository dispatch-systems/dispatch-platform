import { useState } from 'react';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { localDate, shiftDate } from '../../lib/meal-breaks.js';
import {
  paycomDateKey,
  selectedPaycomDate,
  validPaycomDate as validDay,
} from '../../lib/paycom-date.js';
import { DateField } from '../../ui/index.js';

// Collection accepts dates up to the DSP's business date, so a viewer in another
// timezone must see and select the DSP's day rather than their own.
export function usePaycomDate(dspId: string, timezone: string) {
  const today = localDate(timezone);
  const key = paycomDateKey(dspId);
  const [selectedDate, setDate] = useState(() => selectedPaycomDate(dspId, timezone));
  const date = validDay(selectedDate, today) ? selectedDate : today;
  const selectDate = (value: string) => {
    if (!validDay(value, today)) return;
    setDate(value);
    try {
      sessionStorage.setItem(key, value);
    } catch {
      // The shared in-memory selection remains available across tabs.
    }
  };
  return { date, today, selectDate };
}

export function PaycomDateControls({
  date,
  today,
  onChange,
}: {
  date: string;
  today: string;
  onChange: (date: string) => void;
}) {
  const select = (value: string) => {
    if (validDay(value, today)) onChange(value);
  };
  return (
    <div className="paycom-date-controls paycom-date-controls-compact">
      <button
        className="icon-button"
        aria-label="Previous day"
        disabled={date <= '2000-01-01'}
        onClick={() => select(shiftDate(date, -1))}
      >
        <ChevronLeft size={16} />
      </button>
      <DateField
        label="Paycom date"
        value={date}
        min="2000-01-01"
        max={today}
        today={today}
        onChange={select}
      />
      <button
        className="icon-button"
        aria-label="Next day"
        disabled={date >= today}
        onClick={() => select(shiftDate(date, 1))}
      >
        <ChevronRight size={16} />
      </button>
      <button disabled={date === today} onClick={() => select(today)}>
        Today
      </button>
    </div>
  );
}
