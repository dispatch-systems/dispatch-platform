// What the agent test set's questions are made of. Each owner asks about its own data in its
// tests/mcp/questions.ts, and `npm run agents:eval` asks them all. Each expected answer is read
// straight from the source databases with SQL of its own, never through the agent API, so a
// bug in a tool cannot also make its own answer right.
import { DatabaseSync } from 'node:sqlite';
import path from 'node:path';

export type Grade = 'name' | 'names' | 'number' | 'text' | 'date';
export type Question = {
  id: string;
  ask: string;
  /** How the final line should be written, so it can be checked without a model. */
  format: string;
  grade: Grade;
  /** Every acceptable answer: tied names, or one value. */
  expected: string[];
  /** For numbers, how far off still counts. */
  tolerance?: number;
  /** A marker that must have appeared in a tool result before the answer can pass. */
  requiredToolEvidence?: string;
};
/** The synthetic DSP the questions are about: its data's root, and the fortnight it holds. */
export type World = { root: string; dsp: string; from: string; to: string; timezone: string };

export const day = (date: string, shift: number) => {
  const d = new Date(`${date}T12:00:00Z`);
  d.setUTCDate(d.getUTCDate() + shift);
  return d.toISOString().slice(0, 10);
};
/** One of the DSP's databases, read only. */
export const open = (world: World, name: string) =>
  new DatabaseSync(path.join(world.root, 'dsps', world.dsp, 'data', name, `${name}.sqlite`), {
    readOnly: true,
  });
/** Every name that shares the top value: a tie accepts any of them. */
export const top = (rows: { name: string; value: number }[], lowest = false) => {
  const sorted = [...rows].sort((a, b) => (lowest ? a.value - b.value : b.value - a.value));
  return sorted.filter((row) => row.value === sorted[0]!.value).map((row) => row.name);
};
export const local = (ms: number, timezone: string) =>
  new Intl.DateTimeFormat('en-GB', {
    timeZone: timezone,
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  }).format(new Date(ms));

/** The days and spans the questions name. */
export function days(world: World) {
  const yesterday = world.to;
  const today = day(world.to, 1);
  const weekFrom = day(world.to, -6);
  // Last week as Amazon counts it: the Sunday-to-Saturday week before this one.
  const weekday = new Date(`${today}T12:00:00Z`).getUTCDay();
  const lastSunday = day(today, -weekday - 7);
  const lastSaturday = day(lastSunday, 6);
  return {
    yesterday,
    weekFrom,
    lastSunday,
    lastSaturday,
    fortnight: `from ${world.from} to ${world.to}`,
    week: `from ${weekFrom} to ${yesterday}`,
  };
}
