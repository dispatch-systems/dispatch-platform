import { useState } from 'react';
import { useCachedData, useData, wordedApi } from '../../shell/frontend/runtime/api.js';
import { upload } from '../../shell/frontend/runtime/transfer.js';
import type { SupportDetail, SupportNotification, SupportTicket } from './index.js';
import { attachmentSchema } from './runtime.js';

const call = wordedApi({
  support_ticket_not_found: 'This ticket is unavailable.',
  support_proposal_changed: 'The proposal changed. Read the latest version before deciding.',
  support_attachment_unavailable:
    'This screenshot is no longer available. Remove it and add it again.',
  support_not_awaiting_approval: 'This ticket changed. Refresh it before deciding.',
  support_decision_exists: 'A decision has already been saved for this proposal.',
  support_ticket_closed: 'Reopen this ticket before asking the agent another question.',
  support_ticket_active: 'This ticket is already being worked on.',
  support_not_waiting: 'This work has already resumed. Refresh to see its progress.',
});
export function useTickets(owner = false, status = '', query = '') {
  const parameters = new URLSearchParams();
  if (status) parameters.set('status', status);
  if (query.trim()) parameters.set('q', query.trim());
  const path = owner ? '/api/platform/support/tickets' : '/api/support/tickets';
  const url = `${path}${parameters.size ? `?${parameters}` : ''}`;
  const result = useData<SupportTicket[]>(url, 10000);
  const [older, setOlder] = useState<{ scope: string; rows: SupportTicket[]; more: boolean }>();
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [olderFailure, setOlderFailure] = useState<{ scope: string; message: string }>();
  const old = older?.scope === url ? older : undefined;
  const data = result.data
    ? Array.from(
        new Map(
          [...(old?.rows ?? []), ...result.data].map((ticket) => [ticket.id, ticket]),
        ).values(),
      ).sort((a, b) => b.updatedAt - a.updatedAt || (a.id === b.id ? 0 : a.id < b.id ? 1 : -1))
    : undefined;
  const hasMore = old?.more ?? result.data?.length === 100;
  const loadOlder = async () => {
    const last = data?.at(-1);
    if (!last || loadingOlder) return;
    setLoadingOlder(true);
    setOlderFailure(undefined);
    const cursor = new URLSearchParams(parameters);
    cursor.set('before', String(last.updatedAt));
    cursor.set('beforeId', last.id);
    try {
      const rows = await call<SupportTicket[]>(`${path}?${cursor}`);
      setOlder((previous) => ({
        scope: url,
        rows: [...(previous?.scope === url ? previous.rows : []), ...rows],
        more: rows.length === 100,
      }));
    } catch (cause) {
      setOlderFailure({
        scope: url,
        message: cause instanceof Error ? cause.message : 'Older tickets could not be loaded.',
      });
    } finally {
      setLoadingOlder(false);
    }
  };
  const olderError = olderFailure?.scope === url ? olderFailure.message : '';
  const refresh = () => {
    setOlder(undefined);
    setOlderFailure(undefined);
    result.refresh();
  };
  return { ...result, refresh, data, hasMore, loadOlder, loadingOlder, olderError };
}
export const useTicket = (id: string) =>
  useData<SupportDetail>(id ? `/api/support/tickets/${encodeURIComponent(id)}` : '', 5000);
export const useNotifications = () =>
  useCachedData<SupportNotification[]>('/api/support/notifications', 15000);
export const useUnreadNotifications = () =>
  useCachedData<{ count: number }>('/api/support/notifications/unread', 15000);
export const attachmentUrl = (id: string) => `/api/support/attachments/${encodeURIComponent(id)}`;
export async function uploadScreenshot(file: File) {
  if (file.size > 20 * 1024 * 1024)
    throw new Error('Screenshots must be 20 MB or smaller. Choose a smaller image.');
  return attachmentSchema.parse(
    await upload(`/api/support/attachments?name=${encodeURIComponent(file.name)}`, file),
  );
}
export const createTicket = (body: {
  requestId: string;
  kind: string;
  title: string;
  description: string;
  attachmentIds: string[];
}) => call<SupportDetail>('/api/support/tickets', body);
export const sendMessage = (id: string, body: string, target: 'reporter' | 'agent') =>
  call<SupportDetail>(`/api/support/tickets/${encodeURIComponent(id)}/messages`, { body, target });
export const reopenTicket = (id: string) =>
  call<SupportDetail>(`/api/support/tickets/${encodeURIComponent(id)}/reopen`, {});
export const decideTicket = (
  id: string,
  decision: 'approve' | 'deny',
  version: number,
  note: string,
) =>
  call<SupportDetail>(`/api/platform/support/tickets/${encodeURIComponent(id)}/decision`, {
    decision,
    version,
    note,
  });
export const resumeTicket = (id: string) =>
  call<SupportDetail>(`/api/platform/support/tickets/${encodeURIComponent(id)}/resume`, {});
export const readNotifications = (ids: string[]) =>
  call<{ ok: true }>('/api/support/notifications/read', { ids });
