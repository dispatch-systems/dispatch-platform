import { Check } from 'lucide-react';
import type { ReactNode } from 'react';
import type { Driver, DriverPair } from '../../api/index.js';
import { DriverAvatar } from '../parts/DriverAvatar.js';
import { RoleTag, SourceMark } from '../parts/Tags.js';

const unique = (values: string[]) => [...new Set(values)];
const names = (driver: Driver) => unique(driver.ids.map((id) => id.name).filter(Boolean));
const emails = (driver: Driver) => unique(driver.ids.flatMap((id) => id.emails));
const phones = (driver: Driver) => unique(driver.ids.flatMap((id) => id.phones));
/** A phone number's last ten digits, as Driver Match compares them. */
const digits = (phone: string) => phone.replace(/\D/g, '').slice(-10);
const lower = (text: string) => text.toLocaleLowerCase('en-US');
/** Whether two lists share a value, compared as `key` writes each. */
const shared = (a: string[], b: string[], key: (value: string) => string) =>
  a.some((x) => b.some((y) => key(x) === key(y)));

function Row({
  label,
  person,
  candidate,
  same = false,
}: {
  label: string;
  person: ReactNode;
  candidate: ReactNode;
  same?: boolean;
}) {
  const tone = same ? ' same' : '';
  return (
    <>
      <div className="driver-compare-label">{label}</div>
      <div className={`driver-compare-cell${tone}`}>{person}</div>
      <div className={`driver-compare-mark${tone}`}>
        {same && <Check size={14} aria-label="Same" />}
      </div>
      <div className={`driver-compare-cell${tone}`}>{candidate}</div>
    </>
  );
}
const list = (values: string[]) => (values.length ? values.join(', ') : '—');

/** Both sides of a pair, field by field, with what they share marked. */
export function Comparison({ pair }: { pair: DriverPair }) {
  const { person, candidate } = pair;
  const side = (driver: Driver) => (
    <>
      {driver.ids.map((id) => (
        <SourceMark key={`${id.source}:${id.id}`} source={id.source} small />
      ))}
      <b>{driver.name}</b>
      <span className="driver-code">{driver.code}</span>
    </>
  );
  return (
    <div className="driver-compare">
      <div className="driver-compare-label" />
      <div className="driver-compare-cell head">{side(person)}</div>
      <div className="driver-compare-mark" />
      <div className="driver-compare-cell head">{side(candidate)}</div>
      {(person.photo || candidate.photo) && (
        <Row
          label="Photo"
          person={<DriverAvatar driver={person} large />}
          candidate={<DriverAvatar driver={candidate} large />}
        />
      )}
      <Row
        label="Name"
        person={list(names(person))}
        candidate={list(names(candidate))}
        same={shared(names(person), names(candidate), lower)}
      />
      <Row
        label="Email"
        person={list(emails(person))}
        candidate={list(emails(candidate))}
        same={shared(emails(person), emails(candidate), lower)}
      />
      <Row
        label="Phone"
        person={list(phones(person))}
        candidate={list(phones(candidate))}
        same={shared(phones(person), phones(candidate), digits)}
      />
      <Row
        label="Role"
        person={<RoleTag role={person.role} />}
        candidate={<RoleTag role={candidate.role} />}
      />
    </div>
  );
}
