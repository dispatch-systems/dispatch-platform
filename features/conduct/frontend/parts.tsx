import type { ReactNode } from 'react';
import { PersonAvatar } from '../../../core/shell/frontend/runtime/PersonAvatar.js';
import type {
  ConductOverview,
  ConductPerson,
  ConductTier,
  ConductWriteupRow,
} from '../api/index.js';
import { indexOf } from './data.js';
import { finalNote, printedDay, tierShort } from './tiers.js';

// The small pieces every screen of Conduct draws alike.

/** A tier, colored by how far up it is, with its category's name before it when given. */
export function TierBadge({ tier, category }: { tier: ConductTier; category?: string }) {
  return (
    <span className={`conduct-tier conduct-tier-${tier}`}>
      {category && <span className="conduct-tier-category">{category} ·</span>}
      {tierShort[tier]}
    </span>
  );
}

/** A writeup's tier, with what its final comes with beneath it. */
export function WriteupTier({ writeup }: { writeup: ConductWriteupRow }) {
  const note = writeup.tier === 'final' ? finalNote(writeup.finalKind, writeup.suspension) : '';
  return (
    <>
      <TierBadge tier={writeup.tier} />
      {note && <div className="conduct-note">{note}</div>}
    </>
  );
}

/** Whether a writeup is signed, and when. */
export function SignedState({ writeup }: { writeup: ConductWriteupRow }) {
  return writeup.signedOn ? (
    <span className="conduct-state conduct-state-signed">
      Signed {printedDay(writeup.signedOn)}
    </span>
  ) : (
    <span className="conduct-state conduct-state-waiting">Awaiting signature</span>
  );
}

/** Someone, with their photo or initials. */
export function Person({ person, large = false }: { person: ConductPerson; large?: boolean }) {
  return (
    <span className="conduct-person">
      <PersonAvatar
        name={person.name}
        code={person.left ? null : person.code}
        photo={person.photo}
        className={large ? 'conduct-avatar conduct-avatar-large' : 'conduct-avatar'}
      />
      <span>{person.name}</span>
    </span>
  );
}

/** The person `code` names, as the page's data knows them. */
export const personOf = (overview: ConductOverview, code: string): ConductPerson =>
  indexOf(overview).people.get(code) ?? { code, name: code, photo: null, left: true };

/** The name of the category `id`. */
export const categoryName = (overview: ConductOverview, id: string) =>
  indexOf(overview).categories.get(id)?.name ?? id;

/** What opens a table's row, for the keyboard: the row itself opens it for a pointer. */
export function RowLink({ open, children }: { open: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      className="conduct-row-link"
      onClick={(event) => {
        event.stopPropagation();
        open();
      }}
    >
      {children}
    </button>
  );
}
