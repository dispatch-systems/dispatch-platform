import type { ReactNode } from 'react';
import type { Timecard } from '../../../../../shared/contracts/index.js';
import { paycomColumns, type PaycomColumn } from '../../paycom.js';
import { Badge, type TableColumn } from '../../../../../core/shell/frontend/ui/index.js';

const punches = (card: Timecard, values: (string | null)[]) =>
  card.punches.length > 1 ? values.map((value) => value ?? '—') : ['—'];
const lunchOuts = (card: Timecard) =>
  punches(
    card,
    card.punches.slice(0, -1).map((punch) => punch.out),
  );
const lunchIns = (card: Timecard) =>
  punches(
    card,
    card.punches.slice(1).map((punch) => punch.in),
  );
const values: Record<PaycomColumn, (card: Timecard) => string> = {
  inDay: (card) => card.punches[0]?.in ?? '—',
  outLunch: (card) => lunchOuts(card).join(', '),
  inLunch: (card) => lunchIns(card).join(', '),
  outDay: (card) => card.punches.at(-1)?.out ?? '—',
  totalHours: (card) => card.hours.toFixed(2),
  condition: (card) => card.status,
};
const PunchTimes = ({ times }: { times: string[] }) => (
  <span className="punch-times">
    {times.map((time, index) => (
      <span key={index}>{time}</span>
    ))}
  </span>
);
const cells: Partial<Record<PaycomColumn, (card: Timecard) => ReactNode>> = {
  outLunch: (card) => <PunchTimes times={lunchOuts(card)} />,
  inLunch: (card) => <PunchTimes times={lunchIns(card)} />,
  condition: (card) => (
    <Badge value={card.status.toLowerCase() === 'complete' ? 'ready' : 'pending'}>
      {card.status}
    </Badge>
  ),
};
const headerClassNames: Record<PaycomColumn, string> = {
  inDay: 'punch-time-column',
  outLunch: 'punch-time-column',
  inLunch: 'punch-time-column',
  outDay: 'punch-time-column',
  totalHours: 'punch-hours-column',
  condition: 'punch-status-column',
};

/** One column per Paycom punch field, for any table whose rows are timecards. */
export const punchColumns = <T extends Timecard>(sortable: boolean): TableColumn<T>[] =>
  paycomColumns.map(([key, label]) => ({
    id: key,
    header: label,
    sortable,
    headerClassName: headerClassNames[key],
    value: values[key],
    cell: cells[key] ?? values[key],
  }));
