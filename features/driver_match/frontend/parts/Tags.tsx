import { CircleCheck, Mail, Phone } from 'lucide-react';
import type { PeopleSource, IdLink, Person } from '../../api/index.js';
import { links, roleLabel, roles, sourceKinds, sourceLabel } from '../driver-match.js';

/** What a person does for the DSP: their title, else their role, in their role's tone. */
export function RoleTag({ person }: { person: Person }) {
  return <span className={`driver-tag ${roles[person.role].tone}`}>{roleLabel(person)}</span>;
}

const linkIcons: Partial<Record<IdLink, typeof Mail>> = {
  email: Mail,
  phone: Phone,
  person: CircleCheck,
};
/** How an ID came to its person; one a person decided reads in green. */
export function LinkTag({ link }: { link: IdLink }) {
  const Icon = linkIcons[link];
  return (
    <span className={`driver-tag${link === 'person' ? ' sage' : ''}`}>
      {Icon && <Icon size={12} aria-hidden="true" />}
      {links[link].label}
    </span>
  );
}

/** A source, by its name, in its own tone. */
export function SourceTag({ source }: { source: PeopleSource }) {
  const tone = sourceKinds.indexOf(source) % 4;
  return <span className={`driver-source tone-${tone}`}>{sourceLabel(source)}</span>;
}
