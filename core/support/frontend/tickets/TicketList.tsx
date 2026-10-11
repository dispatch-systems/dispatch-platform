import { Bug, ChevronRight, Lightbulb } from 'lucide-react';
import type { SupportTicket } from '../../api/index.js';
import { Empty } from '../../../shell/frontend/ui/index.js';
import { TicketStatus, when } from '../format.js';
export function TicketList({
  tickets,
  selected,
  onSelect,
}: {
  tickets: SupportTicket[];
  selected?: string;
  onSelect: (id: string) => void;
}) {
  if (!tickets.length)
    return (
      <Empty title="No tickets yet">Your reports and feature requests will appear here.</Empty>
    );
  return (
    <div className="support-ticket-list" role="region" aria-label="Support tickets">
      {tickets.map((ticket) => (
        <button
          type="button"
          className={`support-ticket-row ${selected === ticket.id ? 'selected' : ''}`}
          key={ticket.id}
          onClick={() => onSelect(ticket.id)}
          aria-pressed={selected === ticket.id}
        >
          <span className={`support-kind-icon ${ticket.kind}`}>
            {ticket.kind === 'bug' ? <Bug size={17} /> : <Lightbulb size={17} />}
          </span>
          <span className="support-ticket-copy">
            <strong>{ticket.title}</strong>
            <span>
              {ticket.reporterName}
              {ticket.dspName ? ` · ${ticket.dspName}` : ''}
            </span>
            <TicketStatus ticket={ticket} />
          </span>
          <span className="support-ticket-age">
            {when(ticket.updatedAt)}
            <ChevronRight size={16} />
          </span>
        </button>
      ))}
    </div>
  );
}
