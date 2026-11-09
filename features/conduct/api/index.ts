// Conduct's API types, as its screens use them.
export type { ConductCategory } from './generated/ConductCategory';
export type { ConductFinalKind } from './generated/ConductFinalKind';
export type { ConductOverview } from './generated/ConductOverview';
export type { ConductPerson } from './generated/ConductPerson';
export type { ConductSaved } from './generated/ConductSaved';
export type { ConductStanding } from './generated/ConductStanding';
export type { ConductTier } from './generated/ConductTier';
export type { ConductWriteup } from './generated/ConductWriteup';
export type { ConductWriteupRow } from './generated/ConductWriteupRow';

import type { ConductFinalKind } from './generated/ConductFinalKind';
import type { ConductTier } from './generated/ConductTier';

/** A writeup as the page sends it: `revision` for an edit. */
export interface WriteupInput {
  person: string;
  category: string;
  tier: ConductTier;
  finalKind: ConductFinalKind | null;
  suspension: string | null;
  writtenOn: string;
  statement: string;
  prior: string;
  plan: string;
  revision?: number;
}

/** A category as the page sends it: `revision` for an edit. */
export interface CategoryInput {
  name: string;
  startTier: ConductTier;
  statement: string;
  prior: string;
  plan: string;
  revision?: number;
}
