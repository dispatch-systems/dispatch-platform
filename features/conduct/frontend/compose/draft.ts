import type {
  ConductOverview,
  ConductTier,
  ConductWriteup,
  WriteupInput,
} from '../../api/index.js';
import { blank, priorLine, record, suggestedTier } from '../tiers.js';
import type { FinalChoice } from './TierPicker.js';

// A writeup as it's being written, and how choosing who it's for and its category fills it.

export type Draft = {
  person: string;
  category: string;
  writtenOn: string;
  tier: ConductTier;
  final: FinalChoice;
  suspension: string;
  statement: string;
  prior: string;
  plan: string;
};

/** What choosing a person and a category fills in. */
export type Filled = Pick<Draft, 'tier' | 'statement' | 'prior' | 'plan'>;

export const emptyDraft = (today: string, person = ''): Draft => ({
  person,
  category: '',
  writtenOn: today,
  tier: 'verbal',
  final: 'none',
  suspension: '',
  statement: '',
  prior: '',
  plan: '',
});

export const draftOf = (writeup: ConductWriteup): Draft => ({
  person: writeup.person,
  category: writeup.categoryId,
  writtenOn: writeup.writtenOn,
  tier: writeup.tier,
  final: writeup.finalKind ?? 'none',
  suspension: writeup.suspension ?? '',
  statement: writeup.statement,
  prior: writeup.prior,
  plan: writeup.plan,
});

/**
 * What a writeup for `person` in `category` starts as: the tier above where they stand in it,
 * and the category's wording, its prior discussion naming their writeups in it since their
 * record there was last clean. For `editing`, where they stood just before it.
 */
export function filled(
  overview: ConductOverview,
  person: string,
  category: string,
  editing?: string,
): Filled | undefined {
  const kind = overview.categories.find((c) => c.id === category);
  if (!person || !kind) return undefined;
  const { stood, earlier } = record(overview, person, category, editing);
  const line = priorLine(kind.name, earlier);
  return {
    tier: suggestedTier(stood, kind.startTier),
    statement: kind.statement,
    prior: line ? `${kind.prior}\n${line}` : kind.prior,
    plan: kind.plan,
  };
}
/** The parts of `draft` choosing a person and a category fills. */
export const filledOf = ({ tier, statement, prior, plan }: Draft): Filled => ({
  tier,
  statement,
  prior,
  plan,
});

/**
 * `draft` filled as `next` says, keeping each part someone changed since it was `before`; and
 * what each part was filled with now, to tell a later change by.
 */
export function refill(
  draft: Draft,
  before: Filled,
  next: Filled | undefined,
): { draft: Draft; before: Filled } {
  if (!next) return { draft, before };
  const untouched = (key: keyof Filled) => draft[key] === before[key];
  const pick = <K extends keyof Filled>(key: K) => (untouched(key) ? next[key] : draft[key]);
  const base = <K extends keyof Filled>(key: K) => (untouched(key) ? next[key] : before[key]);
  return {
    draft: {
      ...draft,
      tier: pick('tier'),
      statement: pick('statement'),
      prior: pick('prior'),
      plan: pick('plan'),
    },
    before: {
      tier: base('tier'),
      statement: base('statement'),
      prior: base('prior'),
      plan: base('plan'),
    },
  };
}

/** Why `draft` can't be saved yet, if it can't. */
export function missing(draft: Draft): string | undefined {
  if (!draft.person) return 'Choose who this writeup is for.';
  if (!draft.category) return 'Choose a category.';
  if (draft.tier === 'final' && draft.final === 'suspension' && !draft.suspension.trim())
    return 'Enter how long the suspension is.';
  const sections = [draft.statement, draft.prior, draft.plan];
  if (sections.some((text) => !text.trim())) return 'Fill in every section.';
  if (sections.some((text) => text.match(blank))) return 'Fill in the blanks marked in yellow.';
  return undefined;
}

/** `draft` as the server takes it. */
export const inputOf = (draft: Draft, revision?: number): WriteupInput => {
  const final = draft.tier === 'final' && draft.final !== 'none' ? draft.final : null;
  return {
    person: draft.person,
    category: draft.category,
    tier: draft.tier,
    finalKind: final,
    suspension: final === 'suspension' ? draft.suspension.trim() : null,
    writtenOn: draft.writtenOn,
    statement: draft.statement,
    prior: draft.prior,
    plan: draft.plan,
    ...(revision === undefined ? {} : { revision }),
  };
};
