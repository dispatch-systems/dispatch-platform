import { Check } from 'lucide-react';
import type { IntegrationNote } from '../../../../core/shell/frontend/runtime/slots.js';

// What an integration says of itself: how it's used, step by step, and its notes, each with its
// mark, a tick when it has none.

export function IntegrationSteps({ steps }: { steps: readonly string[] }) {
  return (
    <ol className="slack-integration-steps">
      {steps.map((step) => (
        <li key={step}>{step}</li>
      ))}
    </ol>
  );
}

export function IntegrationNotes({ notes }: { notes: readonly IntegrationNote[] }) {
  return (
    <ul className="slack-integration-notes">
      {notes.map(({ text, icon: Icon }) => (
        <li key={text} className={Icon ? undefined : 'ticked'}>
          {Icon ? <Icon size={16} aria-hidden="true" /> : <Check size={16} aria-hidden="true" />}
          {text}
        </li>
      ))}
    </ul>
  );
}
