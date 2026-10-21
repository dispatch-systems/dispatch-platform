import type { MouseEvent } from 'react';
import { Clock } from 'lucide-react';
import type { Correspondent, PersonRole, Writing } from '../../api/index.js';
import { personTitles } from '../../../../core/manifest/api/generated/people-data.js';
import { marked } from '../inbox.js';
import { waited } from '../format.js';

// Pieces the list, the conversation and New message share.

/** A name, with the letters that matched what was searched marked. */
export function Name({
  name,
  matched = [],
}: {
  name: string;
  matched?: readonly (readonly [number, number])[];
}) {
  return (
    <>
      {marked(name, matched).map((run, at) =>
        run.mark ? <mark key={at}>{run.text}</mark> : run.text,
      )}
    </>
  );
}

/** How long someone has waited on an answer: amber from 15 minutes, red from 30. */
export function Wait({ since, now }: { since: number; now: number }) {
  const { label, tone } = waited(since, now);
  return (
    <span className={tone ? `wait ${tone}` : 'wait'}>
      <Clock size={11} aria-hidden="true" />
      {label}
    </span>
  );
}

/** "Keisha is typing…", for those at the DSP writing in a conversation. */
export function typingNote(writing: readonly Writing[]) {
  const names = writing.map((writer) => writer.firstName);
  const who =
    names.length > 2 ? `${names.slice(0, -1).join(', ')} and ${names.at(-1)}` : names.join(' and ');
  return `${who} ${names.length > 1 ? 'are' : 'is'} typing…`;
}

/** What someone does for the DSP, as Driver Match knows them. */
const roles: Record<PersonRole, string> = {
  driver: 'Driver',
  helper: 'Helper',
  office: 'Management',
  former: 'Former',
};
/** What someone does, as it reads: their title, else their role; nothing when unknown. */
export const roleText = ({ role, title }: Pick<Correspondent, 'role' | 'title'>) =>
  personTitles.find((t) => t.id === title)?.label ?? (role && roles[role]);

/** What makes a button open the profile `id` beside a conversation and close it again. */
export function opensProfile(id: string, open: boolean, toggle: (from: HTMLElement) => void) {
  return {
    'data-opens-profile': '',
    'aria-haspopup': 'dialog',
    'aria-expanded': open,
    'aria-controls': open ? id : undefined,
    onClick: (event: MouseEvent<HTMLElement>) => toggle(event.currentTarget),
  } as const;
}
export type OpensProfile = ReturnType<typeof opensProfile>;
