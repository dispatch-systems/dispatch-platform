import { CircleAlert, CircleCheck, Mail, Phone } from 'lucide-react';
import type { DriverLink, DriverRole, DriverSource } from '../../api/index.js';
import { linkLabels, roleLabels, roleTones, sourceKinds, sourceMark } from '../driver-match.js';

/** What a person does for the DSP. */
export function RoleTag({ role }: { role: DriverRole }) {
  return <span className={`driver-tag ${roleTones[role]}`}>{roleLabels[role]}</span>;
}

const linkIcons: Partial<Record<DriverLink, typeof Mail>> = {
  email: Mail,
  phone: Phone,
  person: CircleCheck,
};
/** How surely a person's IDs are one person's; waiting for a decision, when they are. */
export function LinkTag({ link, review = false }: { link: DriverLink; review?: boolean }) {
  if (review)
    return (
      <span className="driver-tag warning">
        <CircleAlert size={12} aria-hidden="true" />
        Needs review
      </span>
    );
  const Icon = linkIcons[link];
  return (
    <span className={`driver-tag${link === 'person' ? ' sage' : ''}`}>
      {Icon && <Icon size={12} aria-hidden="true" />}
      {linkLabels[link]}
    </span>
  );
}

/** A source's short mark, in its own tone. */
export function SourceMark({ source, small = false }: { source: DriverSource; small?: boolean }) {
  const tone = sourceKinds.indexOf(source) % 4;
  return (
    <span className={`driver-mark tone-${tone}${small ? ' small' : ''}`}>{sourceMark(source)}</span>
  );
}
