import type { ReactNode } from 'react';
import { Phone, Smartphone } from 'lucide-react';
import type { ConversationKind, Elsewhere } from '../../api/index.js';
import { SlackLogo } from '../../../slack/frontend/index.js';
import { lastFour } from '../shared/format.js';
import './elsewhere-chips.css';

// Whom a conversation is with's conversation of each kind: their work conversation, by the work
// phone they hold now; their own number's, in Messaging; and their Slack one. As chips under its
// head, the one open marked and each other that waits with a dot; each opens its conversation,
// or starts one.

/** What a chip says of a conversation of each kind. */
const shown: Record<ConversationKind, { icon: ReactNode; label: string }> = {
  work: { icon: <Smartphone aria-hidden="true" />, label: 'Work' },
  number: { icon: <Phone aria-hidden="true" />, label: 'Messaging' },
  slack: { icon: <SlackLogo size={13} />, label: 'Slack' },
};

/** A chip for a conversation of one kind: its mark, its name, a work one's phone by its last
 * digits, and a dot while it waits. */
export function ElsewhereChip({
  elsewhere,
  current = false,
  go,
}: {
  elsewhere: Elsewhere;
  current?: boolean;
  go: (elsewhere: Elsewhere) => void;
}) {
  const { icon, label } = shown[elsewhere.kind];
  const phone = elsewhere.kind === 'work' && elsewhere.number && ` ${lastFour(elsewhere.number)}`;
  return (
    <button
      className="elsewhere"
      aria-pressed={current}
      onClick={() => {
        if (!current) go(elsewhere);
      }}
    >
      {icon}
      {label}
      {phone}
      {elsewhere.waiting && !current && <span className="dot" aria-label="Waiting" />}
    </button>
  );
}

/** The chips under a conversation's head, while whom it's with has two or more conversations. */
export function ElsewhereChips({
  elsewhere,
  current,
  go,
}: {
  elsewhere: readonly Elsewhere[];
  /** The conversation open. */
  current: string;
  go: (elsewhere: Elsewhere) => void;
}) {
  if (elsewhere.length < 2) return null;
  return (
    <nav className="elsewhere-chips" aria-label="Conversations">
      {elsewhere.map((each) => (
        <ElsewhereChip
          key={each.kind}
          elsewhere={each}
          current={each.conversationId === current}
          go={go}
        />
      ))}
    </nav>
  );
}
