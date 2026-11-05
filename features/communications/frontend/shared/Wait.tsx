import { Clock } from 'lucide-react';
import { waited } from './format.js';
import './wait.css';

/** How long someone has waited on an answer, "31 min", or with `words`, "Waiting 31 min"; read
 * aloud in full, "Waiting 31 minutes". Amber from 15 minutes, red from 30. */
export function Wait({ since, now, words }: { since: number; now: number; words?: boolean }) {
  const { label, spoken, tone } = waited(since, now);
  return (
    <span className={tone ? `wait ${tone}` : 'wait'}>
      <Clock size={11} aria-hidden="true" />
      <span aria-hidden="true">{words ? `Waiting ${label}` : label}</span>
      <span className="sr-only">Waiting {spoken}</span>
    </span>
  );
}
