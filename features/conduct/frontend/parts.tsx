import type { ReactNode } from 'react';
import { PenLine } from 'lucide-react';
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

/**
 * A member as a writeup names them: the photo of the person the directory knows them as, by
 * their `code`, else the initials of the `name` the writeup gave them.
 */
export function MemberAvatar({
  overview,
  code,
  name,
  className,
}: {
  overview: ConductOverview;
  code: string | null;
  name: string;
  className: string;
}) {
  const person = code ? personOf(overview, code) : undefined;
  return (
    <PersonAvatar
      name={name}
      code={person && !person.left ? person.code : null}
      photo={person?.photo}
      className={className}
    />
  );
}

/** Who wrote a writeup, by name beside their photo or initials. */
export function Supervisor({
  overview,
  writeup,
}: {
  overview: ConductOverview;
  writeup: ConductWriteupRow;
}) {
  return (
    <span className="conduct-member">
      <MemberAvatar
        overview={overview}
        code={writeup.supervisor}
        name={writeup.supervisorName}
        className="conduct-member-avatar"
      />
      {writeup.supervisorName}
    </span>
  );
}

/** Who marked a writeup signed, by name beside their photo or initials. */
export function MarkedBy({
  overview,
  writeup,
}: {
  overview: ConductOverview;
  writeup: ConductWriteupRow;
}) {
  if (!writeup.signedOn || !writeup.signedByName) return null;
  return (
    <span className="conduct-marked-by">
      Marked by
      <MemberAvatar
        overview={overview}
        code={writeup.signedBy}
        name={writeup.signedByName}
        className="conduct-signer"
      />
      <span className="conduct-marked-by-name">{writeup.signedByName}</span>
    </span>
  );
}

/**
 * A writeup's signature: when it was signed, with who marked it so by their photo or initials,
 * named on hover or focus; or a button that marks it signed where given.
 */
export function WriteupStatus({
  overview,
  writeup,
  markSigned,
}: {
  overview: ConductOverview;
  writeup: ConductWriteupRow;
  markSigned?: (writeup: ConductWriteupRow) => void;
}) {
  const signer = writeup.signedByName;
  if (writeup.signedOn && signer)
    return (
      <span
        className="conduct-signed"
        tabIndex={0}
        aria-label={`Signed ${printedDay(writeup.signedOn)}, marked by ${signer}`}
      >
        <SignedState writeup={writeup} />
        <MemberAvatar
          overview={overview}
          code={writeup.signedBy}
          name={signer}
          className="conduct-signer"
        />
        <span className="conduct-signer-card" role="tooltip">
          Marked by {signer}
        </span>
      </span>
    );
  if (writeup.signedOn || !markSigned) return <SignedState writeup={writeup} />;
  return (
    <button
      type="button"
      className="conduct-small"
      onClick={(event) => {
        event.stopPropagation();
        markSigned(writeup);
      }}
    >
      <PenLine size={14} />
      Mark signed
    </button>
  );
}

/** Someone's photo, or their initials, sized by `className`. */
export function Avatar({ person, className }: { person: ConductPerson; className: string }) {
  return (
    <PersonAvatar
      name={person.name}
      code={person.left ? null : person.code}
      photo={person.photo}
      className={className}
    />
  );
}

/** Someone, with their photo or initials. */
export function Person({ person }: { person: ConductPerson }) {
  return (
    <span className="conduct-person">
      <Avatar person={person} className="conduct-avatar" />
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
