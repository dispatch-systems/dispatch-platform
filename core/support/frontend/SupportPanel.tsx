import { useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Bell, CheckCheck, Sparkles, X } from 'lucide-react';
import type { Access } from '../../shell/frontend/runtime/slots.js';
import { DataState, Empty, ErrorBox, Tabs } from '../../shell/frontend/ui/index.js';
import { useFocusTrap } from '../../shell/frontend/ui/useFocusTrap.js';
import {
  readNotifications,
  useNotifications,
  useTickets,
  useUnreadNotifications,
} from '../api/client.js';
import { ReportForm } from './report/ReportForm.js';
import { TicketList } from './tickets/TicketList.js';
import { TicketDetail } from './tickets/TicketDetail.js';
import { TicketPagination } from './tickets/TicketPagination.js';
import { when } from './format.js';
import './support.css';
import './notifications/notifications.css';

export function SupportPanel({ session, onClose }: Access & { onClose: () => void }) {
  const [tab, setTab] = useState('report');
  const [selected, setSelected] = useState('');
  const [submitted, setSubmitted] = useState(false);
  const [error, setError] = useState('');
  const [marking, setMarking] = useState(false);
  const panel = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const heading = useId();
  const tickets = useTickets();
  const notifications = useNotifications();
  const unreadCount = useUnreadNotifications();
  const unread = notifications.data?.filter((item) => item.readAt === null) ?? [];
  useFocusTrap(panel, { onEscape: onClose });
  useEffect(() => {
    content.current?.scrollTo({ top: 0 });
  }, [tab, selected]);
  const mark = async (ids: string[]) => {
    if (!ids.length) return;
    setError('');
    setMarking(true);
    try {
      await readNotifications(ids);
      notifications.refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Notifications could not be marked read.');
    } finally {
      setMarking(false);
    }
  };
  const openTicket = (id: string) => {
    setSelected(id);
    setTab('tickets');
    setSubmitted(false);
    void mark(
      unread
        .filter((notification) => notification.ticketId === id)
        .map((notification) => notification.id),
    );
  };
  return createPortal(
    <div
      className="support-panel-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={panel}
        className="support-panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby={heading}
      >
        <header className="support-panel-header">
          <div className="support-panel-brand">
            <span>
              <Sparkles size={19} />
            </span>
            <div>
              <h2 id={heading}>AI Support</h2>
              <p className="muted">Help shape Dispatch</p>
            </div>
          </div>
          <button className="icon-button" aria-label="Close AI Support" onClick={onClose}>
            <X size={19} />
          </button>
        </header>
        <Tabs
          value={tab}
          onChange={(value) => {
            setTab(value);
            setSubmitted(false);
          }}
          label="AI Support"
          items={[
            ['report', 'New report'],
            ['tickets', 'My tickets'],
            [
              'notifications',
              <span key="notifications" className="support-notifications-tab">
                <Bell size={14} />
                Updates
                {Boolean(unreadCount.data?.count) && (
                  <span className="support-unread-count">{unreadCount.data?.count}</span>
                )}
              </span>,
            ],
          ]}
        />
        <div className="support-panel-body" ref={content}>
          <div hidden={tab !== 'report'}>
            <ReportForm
              onSubmitted={(detail) => {
                tickets.refresh();
                notifications.refresh();
                setSelected(detail.ticket.id);
                setSubmitted(true);
                setTab('tickets');
              }}
            />
          </div>
          {tab === 'tickets' && (
            <>
              {submitted && (
                <p className="support-submitted" role="status">
                  <CheckCheck size={18} />
                  Report received. The agent will investigate and keep you updated here.
                </p>
              )}
              {selected ? (
                <TicketDetail
                  id={selected}
                  owner={session.user.platformOwner}
                  onBack={() => {
                    setSelected('');
                    setSubmitted(false);
                  }}
                  onChanged={() => {
                    tickets.refresh();
                    notifications.refresh();
                  }}
                />
              ) : (
                <DataState data={tickets.data} error={tickets.error} retry={tickets.refresh}>
                  {(data) => (
                    <>
                      <TicketList tickets={data} onSelect={openTicket} />
                      <TicketPagination {...tickets} />
                    </>
                  )}
                </DataState>
              )}
            </>
          )}
          {tab === 'notifications' && (
            <>
              <div className="support-updates-heading">
                <h3>Updates</h3>
                {unread.length > 0 && (
                  <button
                    className="text-button"
                    disabled={marking}
                    onClick={() => void mark(unread.map((item) => item.id))}
                  >
                    {(unreadCount.data?.count ?? 0) > unread.length
                      ? 'Mark displayed read'
                      : 'Mark all read'}
                  </button>
                )}
              </div>
              <ErrorBox message={error} />
              <DataState
                data={notifications.data}
                error={notifications.error}
                retry={notifications.refresh}
              >
                {(data) =>
                  data.length ? (
                    <ol className="support-notification-list">
                      {data.map((notification) => (
                        <li key={notification.id}>
                          <button onClick={() => openTicket(notification.ticketId)}>
                            <span
                              className={`support-notification-dot ${notification.readAt === null ? 'unread' : ''}`}
                            />
                            <span>
                              <strong>{notification.body}</strong>
                              <span className="muted">{when(notification.createdAt)}</span>
                            </span>
                          </button>
                        </li>
                      ))}
                    </ol>
                  ) : (
                    <Empty title="You're all caught up">
                      Ticket progress, replies and decisions will appear here.
                    </Empty>
                  )
                }
              </DataState>
            </>
          )}
        </div>
        <footer className="support-panel-footer">
          <span>Visible to you and the platform owner</span>
          <span>Dispatch</span>
        </footer>
      </div>
    </div>,
    document.body,
  );
}
