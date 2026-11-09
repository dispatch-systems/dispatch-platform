import { Fragment } from 'react';
import type { ConductFinalKind, ConductTier } from '../../api/index.js';
import { blank, finalNote, printedDay, tierName, tiers } from '../tiers.js';
import * as wording from './wording.js';

/** What a notice prints: a writeup, saved or being written. */
export type NoticeContent = {
  personName: string;
  supervisorName: string;
  writtenOn: string;
  tier: ConductTier;
  finalKind: ConductFinalKind | null;
  suspension: string | null;
  statement: string;
  prior: string;
  plan: string;
};

/** A section's words, each blank still to fill marked while it's being written. */
function Answer({ text, marked }: { text: string; marked: boolean }) {
  if (!marked) return <div className="conduct-notice-answer">{text}</div>;
  const parts = text.split(blank);
  const blanks = text.match(blank) ?? [];
  return (
    <div className="conduct-notice-answer">
      {parts.map((part, at) => (
        <Fragment key={at}>
          {part}
          {blanks[at] && <mark>{blanks[at]}</mark>}
        </Fragment>
      ))}
    </div>
  );
}

/**
 * The Corrective Action Notice as it prints, on a sheet of paper: the driver signs it there.
 * `marked` marks the blanks left to fill, on screen while it's being written.
 */
export function Notice({ content, marked = false }: { content: NoticeContent; marked?: boolean }) {
  const note = content.tier === 'final' ? finalNote(content.finalKind, content.suspension) : '';
  return (
    <article className="conduct-notice" aria-label={wording.title}>
      <h2>{wording.title}</h2>
      <div className="conduct-notice-top">
        <span>
          Employee: <u>{content.personName}</u>
        </span>
        <span>
          Supervisor: <u>{content.supervisorName}</u>
        </span>
        <span>
          Date: <u>{printedDay(content.writtenOn)}</u>
        </span>
      </div>
      <div className="conduct-notice-tiers">
        <b>{wording.typeHeading}</b>
        {tiers.map((tier) => (
          <div key={tier}>
            <span className="conduct-notice-box">{content.tier === tier ? 'X' : ''}</span>
            {tierName[tier]}
            {tier === 'final' && note && <small>{note}</small>}
          </div>
        ))}
      </div>
      <p className="conduct-notice-disclaimer">
        <b>*</b> {wording.disclaimer}
      </p>
      <ol>
        <li>
          {wording.sections.statement}
          <Answer text={content.statement} marked={marked} />
        </li>
        <li>
          {wording.sections.prior}
          <Answer text={content.prior} marked={marked} />
        </li>
        <li>
          {wording.sections.plan}
          <Answer text={content.plan} marked={marked} />
        </li>
        <li>
          {wording.sections.consequences}
          <p className="conduct-notice-consequences">{wording.consequences}</p>
        </li>
      </ol>
      <div className="conduct-notice-signed">
        <span className="conduct-notice-signature">{content.supervisorName}</span>
        <span>{printedDay(content.writtenOn)}</span>
      </div>
      <div className="conduct-notice-labels">
        <span>{wording.supervisorSignature}</span>
        <span>Date</span>
      </div>
      <div className="conduct-notice-comments">
        <b>{wording.employeeComments}</b>
        <div />
        <div />
        <div />
      </div>
      <div className="conduct-notice-acknowledge">
        <span>{wording.acknowledgement}</span>
        <span />
      </div>
      <div className="conduct-notice-acknowledge-labels">
        <span>EMPLOYEE SIGNATURE</span>
        <span>DATE</span>
      </div>
    </article>
  );
}
