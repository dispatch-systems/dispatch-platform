import type {
  ConductFinalKind,
  ConductOverview,
  ConductStanding,
  ConductTier,
  ConductWriteupRow,
} from '../api/index.js';

// How tiers read, climb and step down, and the dates and lines a writeup prints: shared by
// every screen. The server works out where everyone stands; these read what it says.

/** Every tier, lowest first. */
export const tiers: readonly ConductTier[] = ['verbal', 'written', 'final'];

/** A tier as a list or a button says it. */
export const tierShort: Record<ConductTier, string> = {
  verbal: 'Verbal',
  written: 'Written',
  final: 'Final',
};
/** A tier as the notice prints it. */
export const tierName: Record<ConductTier, string> = {
  verbal: 'Verbal Warning',
  written: 'Written Warning',
  final: 'Final Written Warning',
};

/** What a final writeup comes with, as the notice prints it beside its box. */
export function finalNote(kind: ConductFinalKind | null, suspension: string | null) {
  if (kind === 'suspension') return `w/ ${suspension ?? ''} suspension`;
  if (kind === 'investigation') return 'pending further investigation';
  return '';
}

/** A day, YYYY-MM-DD, as the notice prints it: 8/23/2026. */
export function printedDay(day: string) {
  const [year, month, date] = day.split('-');
  return `${Number(month)}/${Number(date)}/${year}`;
}

/** The tier to suggest for someone in a category: the next above where they stand, never
 * below where the category starts, and Final once they're at Final. */
export function suggestedTier(standing: ConductTier | undefined, start: ConductTier) {
  const next = standing ? Math.min(tiers.indexOf(standing) + 1, tiers.length - 1) : 0;
  return tiers[Math.max(next, tiers.indexOf(start))]!;
}

/**
 * Where `person` stands in `category` for a writeup being written, and their writeups in it
 * since their record there was last clean, oldest first. For the writeup `editing`, in its own
 * person and category, where they stood just before it.
 */
export function record(
  overview: ConductOverview,
  person: string,
  category: string,
  editing?: string,
): { stood: ConductTier | undefined; earlier: ConductWriteupRow[] } {
  const theirs = overview.writeups.filter((w) => w.person === person && w.categoryId === category);
  const at = editing ? theirs.findIndex((w) => w.id === editing) : -1;
  const stood =
    at >= 0
      ? overview.stoodBefore[editing!]
      : overview.standings.find((s) => s.person === person && s.categoryId === category)?.tier;
  const earlier: ConductWriteupRow[] = [];
  if (stood) {
    // Back from the newest, to the one written while their record was clean.
    for (const w of theirs.slice(at + 1)) {
      earlier.push(w);
      if (!overview.stoodBefore[w.id]) break;
    }
  }
  return { stood, earlier: earlier.reverse() };
}

/** When someone next steps down in a category, if they ever do. */
export function stepDown({ stepsDownOn, stepsDownTo }: ConductStanding) {
  if (!stepsDownOn) return '';
  const on = printedDay(stepsDownOn);
  return stepsDownTo ? `Steps down to ${tierShort[stepsDownTo]} on ${on}` : `Resets on ${on}`;
}

/** The line a new writeup's prior discussion ends with, naming the earlier writeups. */
export function priorLine(categoryName: string, earlier: readonly ConductWriteupRow[]) {
  if (earlier.length === 0) return '';
  const each = earlier.map((w) => `${tierName[w.tier]} ${printedDay(w.writtenOn)}`);
  return `Previous writeups for ${categoryName}: ${each.join(', ')}.`;
}

/** The blanks a category's wording leaves to fill, as `[…]`. */
export const blank = /\[[^\]\n]+\]/g;
