const outcomes: Record<string, { title: string; description: string; action: string }> = {
  acknowledged: {
    title: 'Message acknowledged',
    description:
      'Read the agent’s acknowledgment in the conversation below. You can reply if you need help or have more to add.',
    action: 'Ask a follow-up',
  },
  answered: {
    title: 'Question answered',
    description: 'Read the agent’s answer in the conversation below. Reply if you need more help.',
    action: 'Ask a follow-up',
  },
  unclear_bug: {
    title: 'Clarification needed',
    description:
      'The agent investigated but still needs details to identify the bug. Read its response and questions in the conversation and share what you did, what happened and what you expected.',
    action: 'Add details',
  },
  unclear_feature: {
    title: 'Clarification needed',
    description:
      'The agent investigated but still needs a clearer feature description. Read its response and questions in the conversation and describe what you want to do and how it would help.',
    action: 'Add details',
  },
};

export function IntakeOutcome({
  status,
  onReply,
  disabled,
}: {
  status: string;
  onReply: () => void;
  disabled: boolean;
}) {
  const outcome = outcomes[status];
  if (!outcome) return null;
  return (
    <section className="support-section support-intake-outcome" aria-label="Agent response">
      <h3>{outcome.title}</h3>
      <p className="muted">{outcome.description}</p>
      <button type="button" disabled={disabled} onClick={onReply}>
        {outcome.action}
      </button>
    </section>
  );
}
