import { Clock } from 'lucide-react';
import { waited } from './format.js';
import './wait.css';

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
