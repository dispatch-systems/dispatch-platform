import { useUnreadNotifications } from '../../api/client.js';
import './notifications.css';
export function UnreadIndicator() {
  const { data } = useUnreadNotifications();
  const unread = data?.count ?? 0;
  return unread > 0 ? (
    <span className="support-unread-count" aria-label={`${unread} unread AI Support notifications`}>
      {unread}
    </span>
  ) : null;
}
