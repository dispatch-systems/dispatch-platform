import { CircleCheck, Mail, Phone } from 'lucide-react';
import type { DriverLink, DriverRole, DriverSource } from '../../api/index.js';
import { linkLabels, roleLabels, roleTones, sourceKinds, sourceLabel } from '../driver-match.js';

/** What a person does for the DSP. */
export function RoleTag({ role }: { role: DriverRole }) {
  return <span className={`driver-tag ${roleTones[role]}`}>{roleLabels[role]}</span>;
}

const linkIcons: Partial<Record<DriverLink, typeof Mail>> = {
  email: Mail,
  phone: Phone,
  person: CircleCheck,
};
/** How surely an ID is its person's. */
export function LinkTag({ link }: { link: DriverLink }) {
  const Icon = linkIcons[link];
  return (
    <span className={`driver-tag${link === 'person' ? ' sage' : ''}`}>
      {Icon && <Icon size={12} aria-hidden="true" />}
      {linkLabels[link]}
    </span>
  );
}

/** A source, by its name, in its own tone. */
export function SourceTag({ source }: { source: DriverSource }) {
  const tone = sourceKinds.indexOf(source) % 4;
  return <span className={`driver-source tone-${tone}`}>{sourceLabel(source)}</span>;
}
