import { Bug, ChevronRight, Lightbulb } from 'lucide-react';
import type { SupportTicket } from '../../api/index.js';
import { Empty } from '../../../shell/frontend/ui/index.js';
import { kindLabel, TicketStatus, when } from '../format.js';
export function TicketList({
  tickets,
  selected,
  onSelect,
  cards = false,
}: {
  tickets: SupportTicket[];
  selected?: string;
  onSelect: (id: string) => void;
  cards?: boolean;
}) {
  if (!tickets.length)
    return (
      <Empty title="No tickets yet">Your reports and feature requests will appear here.</Empty>
    );
  return (
    <div
      className={`support-ticket-list ${cards ? 'support-ticket-cards' : ''}`}
      role="region"
      aria-label="Support tickets"
    >
      {tickets.map((ticket) => (
        <button
          type="button"
          className={`support-ticket-row ${selected === ticket.id ? 'selected' : ''}`}
          key={ticket.id}
          onClick={() => onSelect(ticket.id)}
          aria-pressed={selected === ticket.id}
        >
          {!cards && (
            <span className={`support-kind-icon ${ticket.kind}`}>
              {ticket.kind === 'bug' ? <Bug size={17} /> : <Lightbulb size={17} />}
            </span>
          )}
          <span className="support-ticket-copy">
            {cards && <TicketStatus ticket={ticket} />}
            <strong>{ticket.title}</strong>
            <span>
              {cards ? (
                <>
                  {kindLabel(ticket.kind)} · Updated {when(ticket.updatedAt)}
                </>
              ) : (
                <>
                  {ticket.reporterName}
                  {ticket.dspName ? ` · ${ticket.dspName}` : ''}
                </>
              )}
            </span>
            {!cards && <TicketStatus ticket={ticket} />}
          </span>
          <span className="support-ticket-age">
            {!cards && when(ticket.updatedAt)}
            <ChevronRight size={16} />
          </span>
        </button>
      ))}
    </div>
  );
}
