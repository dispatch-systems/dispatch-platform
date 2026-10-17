import { Split } from 'lucide-react';
import type { Person, PersonId } from '../../api/index.js';
import { confirmLink } from '../../api/client.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { ErrorBox } from '../../../../core/shell/frontend/ui/index.js';
import { phoneText, seenRange, shortId, sourceLabel } from '../driver-match.js';
import { LinkTag, SourceTag } from '../parts/Tags.js';

/**
 * A person's IDs, each with what its source says of them and how it came to them: a link a
 * match made can be confirmed, and any ID split off while they hold more than one.
 */
export function SheetIds({
  person,
  today,
  onSplit,
}: {
  person: Person;
  today: string;
  onSplit: (id: PersonId) => void;
}) {
  const confirm = useAction((id: PersonId) => confirmLink(person.code, id.source, id.id), {
    inline: true,
    success: (id) => `${sourceLabel(id.source)} ID ${shortId(id)} is confirmed`,
  });
  return (
    <section className="driver-sheet-section" aria-labelledby="driver-ids">
      <h3 id="driver-ids">IDs</h3>
      <ErrorBox message={confirm.error} />
      {person.ids.map((id) => (
        <div className="driver-id" key={`${id.source}:${id.id}`}>
          <div>
            <div className="driver-id-head">
              <SourceTag source={id.source} />
              <strong className="driver-source-id" title={id.id}>
                {shortId(id)}
              </strong>
            </div>
            <small>{[id.name, id.department].filter(Boolean).join(' · ')}</small>
            {id.emails.length + id.phones.length > 0 && (
              <small className="driver-contacts">
                {id.emails.map((email) => (
                  <span key={email}>{email}</span>
                ))}
                {id.phones.map((phone) => (
                  <span key={phone} className="driver-phone">
                    {phoneText(phone)}
                  </span>
                ))}
              </small>
            )}
            {(id.firstSeen || !id.listed) && (
              <small>{seenRange(id.firstSeen, id.lastSeen, today)}</small>
            )}
          </div>
          <LinkTag link={id.linkedBy} />
          {(id.confirmable || person.ids.length > 1) && (
            <div className="driver-id-actions">
              {id.confirmable && (
                <button disabled={confirm.busy} onClick={() => void confirm.run(id)}>
                  Confirm
                </button>
              )}
              {person.ids.length > 1 && (
                <button onClick={() => onSplit(id)}>
                  <Split size={14} aria-hidden="true" />
                  Split off
                </button>
              )}
            </div>
          )}
        </div>
      ))}
    </section>
  );
}
