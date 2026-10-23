import type { SettingsField, SettingsSection } from '../../../../core/settings/api/index.js';
import type { ListedPage } from '../pages.js';

// Settings' search: what a member types, matched against what each page of theirs holds: its
// name, its sections' names, their fields' and options' labels, the words they are also known
// by, the collectors their data comes from, and the values held as text. Every word typed must
// begin a word of one of those, in any case, as "sched" begins "schedules". A page is found
// once, by the best of what matched on it; a page a feature puts under Settings, by its name.

/** Text, with what a search matched in it marked. */
export type Marked = readonly { text: string; mark?: true }[];
/** A page the search found, and what on it matched. */
export type Found = {
  page: ListedPage;
  /** The page's name, marked where it matched. */
  name: Marked;
  /** What on it matched, or, when its name did, the sections it holds. */
  what?: Marked;
  /** The section what matched is in, to point out as the page opens. */
  section?: string;
};

/**
 * How well what matched names a page, best first: its name; a section's; a field's, one of its
 * options' or a word the section is also known by; a collector its data comes from; a value.
 */
const rank = { page: 0, section: 1, field: 2, source: 3, value: 4 } as const;
type Match = { rank: number; what?: Marked; section?: string };

/** The pages `query` finds among those given, best first, else in their order. */
export function search(
  pages: readonly ListedPage[],
  sections: readonly SettingsSection[],
  query: string,
): Found[] {
  const typed = words(query);
  if (!typed.length) return [];
  return pages
    .flatMap((page, order) => {
      const on = sections.filter((section) => section.page === page.id);
      const name = marked(page.label, typed);
      const match: Match | undefined = name
        ? { rank: rank.page, what: listed(page, on) }
        : best(on.flatMap((section) => matches(section, typed)));
      return match ? [{ page, name: name ?? plain(page.label), match, order }] : [];
    })
    .sort((a, b) => a.match.rank - b.match.rank || a.order - b.order)
    .map(({ page, name, match }) => ({ page, name, what: match.what, section: match.section }));
}

/** Everything in a section that `typed` matches, each with how it reads in the results. */
function matches(section: SettingsSection, typed: readonly string[]): Match[] {
  const found: Match[] = [];
  const add = (by: number, text: string, reads: (mark: Marked) => Marked = (mark) => mark) => {
    const mark = marked(text, typed);
    if (mark) found.push({ rank: by, what: reads(mark), section: section.id });
  };
  add(rank.section, section.label);
  for (const field of section.fields) {
    const of = (mark: Marked) => [...plain(`${field.label}: `), ...mark];
    add(rank.field, field.label);
    for (const option of options(field)) add(rank.field, option, of);
    const value = section.values[field.id];
    if (field.kind === 'text' && typeof value === 'string' && value) add(rank.value, value, of);
  }
  for (const keyword of section.keywords)
    add(rank.field, keyword.charAt(0).toUpperCase() + keyword.slice(1));
  for (const source of section.sources)
    add(rank.source, source, (mark) => [...plain(`${section.label} from `), ...mark]);
  return found;
}

/**
 * The labels of what a field may be set to, whatever its kind: its options, its presets, and
 * what none is called.
 */
function options(field: SettingsField) {
  const offered = [
    ...('options' in field ? field.options : []),
    ...('presets' in field ? field.presets : []),
  ];
  const none = 'none' in field && field.none ? [field.none] : [];
  return [...offered.map((option) => option.label), ...none];
}

/** The first of the best matches. */
const best = (found: readonly Match[]) =>
  found.reduce<Match | undefined>(
    (one, other) => (one && one.rank <= other.rank ? one : other),
    undefined,
  );

/** A page's sections, but one named as the page is, as the line under a page whose name matched. */
function listed(page: ListedPage, on: readonly SettingsSection[]) {
  const labels = on.map((section) => section.label).filter((label) => label !== page.label);
  return labels.length ? plain(labels.join(', ')) : undefined;
}

/** The words of `text`, in lower case. */
const words = (text: string) => text.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? [];
const plain = (text: string): Marked => [{ text }];

/** `text` with each word typed marked where it begins a word of it; none unless every one does. */
function marked(text: string, typed: readonly string[]): Marked | undefined {
  const spans: [number, number][] = [];
  for (const word of typed) {
    const at = [...text.matchAll(/[\p{L}\p{N}]+/gu)].find(
      (each) => each[0].slice(0, word.length).toLowerCase() === word,
    )?.index;
    if (at === undefined) return undefined;
    spans.push([at, at + word.length]);
  }
  const parts: { text: string; mark?: true }[] = [];
  let from = 0;
  for (const [start, end] of spans.sort((a, b) => a[0] - b[0])) {
    if (end <= from) continue;
    const begin = Math.max(start, from);
    if (begin > from) parts.push({ text: text.slice(from, begin) });
    parts.push({ text: text.slice(begin, end), mark: true });
    from = end;
  }
  if (from < text.length) parts.push({ text: text.slice(from) });
  return parts;
}
