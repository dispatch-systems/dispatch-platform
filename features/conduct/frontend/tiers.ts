import type {
  ConductFinalKind,
  ConductOverview,
  ConductTier,
  ConductWriteupRow,
} from '../api/index.js';

// How tiers read and climb, and the dates and lines a writeup prints: shared by every screen.

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

/** The highest tier among `writeups`, if any. */
export const highestTier = (writeups: readonly ConductWriteupRow[]) =>
  writeups.reduce<ConductTier | undefined>(
    (high, w) => (high && tiers.indexOf(high) >= tiers.indexOf(w.tier) ? high : w.tier),
    undefined,
  );

/** `person`'s writeups in `category` that still count toward the next tier, oldest first,
 * leaving out the one being edited. */
export function counted(
  overview: ConductOverview,
  person: string,
  category: string,
  except?: string,
): ConductWriteupRow[] {
  return overview.writeups
    .filter(
      (w) =>
        w.person === person &&
        w.categoryId === category &&
        w.id !== except &&
        w.writtenOn >= overview.countsFrom,
    )
    .reverse();
}

/** The line a new writeup's prior discussion ends with, naming the earlier writeups. */
export function priorLine(categoryName: string, earlier: readonly ConductWriteupRow[]) {
  if (earlier.length === 0) return '';
  const each = earlier.map((w) => `${tierName[w.tier]} ${printedDay(w.writtenOn)}`);
  return `Previous writeups for ${categoryName}: ${each.join(', ')}.`;
}

/** The blanks a category's wording leaves to fill, as `[…]`. */
export const blank = /\[[^\]\n]+\]/g;
