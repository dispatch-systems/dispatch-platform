import { Check } from 'lucide-react';
import type { ReactNode } from 'react';
import { phoneText } from '../../../../core/shell/frontend/lib/format.js';
import { phoneKey, roles } from '../driver-match.js';
import { Face } from '../parts/Face.js';
import { SourceTag } from '../parts/Tags.js';
import type { CheckSide } from './checks.js';

const unique = <T,>(values: T[]) => [...new Set(values)];
const emails = (side: CheckSide) => unique(side.ids.flatMap((id) => id.emails));
const phones = (side: CheckSide) => unique(side.ids.flatMap((id) => id.phones));
const lower = (text: string) => text.toLocaleLowerCase('en-US');
/** Whether two lists share a value, compared as `key` writes each; one it can't write, never. */
const shared = (a: string[], b: string[], key: (value: string) => string | undefined) =>
  a.some((x) => key(x) !== undefined && b.some((y) => key(x) === key(y)));

/** An email address that wraps, when it must, before its `@`. */
function Email({ address }: { address: string }) {
  const at = address.lastIndexOf('@');
  return (
    <span>
      {at > 0 ? (
        <>
          {address.slice(0, at)}
          <wbr />
          {address.slice(at)}
        </>
      ) : (
        address
      )}
    </span>
  );
}
const listed = (values: ReactNode[]) =>
  values.length ? values : <span className="driver-none">—</span>;

function Row({
  label,
  left,
  right,
  same = false,
}: {
  label: string;
  left: ReactNode;
  right: ReactNode;
  same?: boolean;
}) {
  return (
    <div className={`driver-compare-row${same ? ' same' : ''}`}>
      <div className="driver-compare-label">{label}</div>
      <div className="driver-compare-cell">{left}</div>
      <div className="driver-compare-mark">{same && <Check size={17} aria-label="Same" />}</div>
      <div className="driver-compare-cell">{right}</div>
    </div>
  );
}

function Head({ side }: { side: CheckSide }) {
  return (
    <div className="driver-compare-cell">
      <Face person={side} size="portrait" />
      <b>{side.name}</b>
      <span className="driver-sources">
        {unique(side.ids.map((id) => id.source)).map((source) => (
          <SourceTag key={source} source={source} />
        ))}
      </span>
    </div>
  );
}

/** Two people side by side, field by field, with what they share in green. */
export function Comparison({ sides: [left, right] }: { sides: [CheckSide, CheckSide] }) {
  const email = (side: CheckSide) => emails(side).map((at) => <Email key={at} address={at} />);
  const phone = (side: CheckSide) =>
    phones(side).map((number) => <span key={number}>{phoneText(number)}</span>);
  return (
    <div className="driver-compare">
      <div className="driver-compare-row head">
        <div className="driver-compare-label" />
        <Head side={left} />
        <div className="driver-compare-mark" />
        <Head side={right} />
      </div>
      <Row
        label="Email"
        left={listed(email(left))}
        right={listed(email(right))}
        same={shared(emails(left), emails(right), lower)}
      />
      <Row
        label="Phone"
        left={listed(phone(left))}
        right={listed(phone(right))}
        same={shared(phones(left), phones(right), phoneKey)}
      />
      <Row label="Role" left={roles[left.role].label} right={roles[right.role].label} />
    </div>
  );
}
