import { useDeferredValue, useEffect, useState } from 'react';
import { ArrowLeft, Search, Sparkles } from 'lucide-react';
import { useTickets } from '../../api/client.js';
import { hashQuery, replaceHashQuery } from '../../../shell/frontend/runtime/navigation.js';
import { DataState, Header } from '../../../shell/frontend/ui/index.js';
import { TicketList } from '../tickets/TicketList.js';
import { TicketDetail } from '../tickets/TicketDetail.js';
import { TicketPagination } from '../tickets/TicketPagination.js';
import { statusLabels } from '../format.js';
import '../support.css';

export function SupportPage() {
  const [selected, setSelected] = useState(() => hashQuery().get('ticket') ?? '');
  const [status, setStatus] = useState('');
  const [search, setSearch] = useState('');
  const deferredSearch = useDeferredValue(search);
  const tickets = useTickets(true, status, deferredSearch);
  useEffect(() => {
    if (selected) window.scrollTo({ top: 0 });
  }, [selected]);
  useEffect(() => {
    const changed = () => setSelected(hashQuery().get('ticket') ?? '');
    window.addEventListener('hashchange', changed);
    return () => window.removeEventListener('hashchange', changed);
  }, []);
  const open = (id: string) => {
    setSelected(id);
    replaceHashQuery(id ? { ticket: id } : {});
  };
  const review =
    tickets.data?.filter((ticket) => ticket.status === 'awaiting_approval').length ?? 0;
  return (
    <>
      <div className="support-owner-heading">
        <Header title="Support" />
        <p className="muted">
          User messages, agent responses, investigations and approved changes in one place.
        </p>
        <span className="support-inbox-count">{review} ready in this view</span>
      </div>
      <div className={`support-owner-workspace ${selected ? 'support-owner-selected' : ''}`}>
        <section className="support-owner-inbox" aria-label="Support inbox">
          <div className="support-inbox-toolbar">
            <label className="support-search">
              <span className="support-sr-only">Search support tickets</span>
              <Search size={17} />
              <input
                type="search"
                maxLength={180}
                placeholder="Search tickets…"
                value={search}
                onChange={(event) => setSearch(event.target.value)}
              />
            </label>
            <label>
              <span className="support-sr-only">Filter ticket status</span>
              <select value={status} onChange={(event) => setStatus(event.target.value)}>
                <option value="">All tickets</option>
                {Object.entries(statusLabels).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <DataState data={tickets.data} error={tickets.error} retry={tickets.refresh}>
            {(data) => (
              <>
                <TicketList tickets={data} selected={selected} onSelect={open} />
                <TicketPagination {...tickets} />
              </>
            )}
          </DataState>
        </section>
        <div className="support-owner-detail">
          {selected ? (
            <>
              <button className="text-button support-inbox-back" onClick={() => open('')}>
                <ArrowLeft size={16} />
                Back to inbox
              </button>
              <TicketDetail id={selected} owner onChanged={tickets.refresh} />
            </>
          ) : (
            <div className="support-owner-empty">
              <span aria-hidden="true">
                <Sparkles size={28} />
              </span>
              <h2>Select a ticket</h2>
              <p className="muted">
                Read the original message and the agent's response. Reports that need changes
                include findings and a proposal for your review.
              </p>
            </div>
          )}
        </div>
      </div>
    </>
  );
}
