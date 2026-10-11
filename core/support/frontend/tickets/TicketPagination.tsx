import { ErrorBox } from '../../../shell/frontend/ui/index.js';
export function TicketPagination({
  hasMore,
  loadOlder,
  loadingOlder,
  olderError,
}: {
  hasMore: boolean;
  loadOlder: () => Promise<void>;
  loadingOlder: boolean;
  olderError: string;
}) {
  return (
    <div className="support-pagination">
      <ErrorBox message={olderError} />
      {hasMore && (
        <button onClick={() => void loadOlder()} disabled={loadingOlder}>
          {loadingOlder ? 'Loading…' : 'Load older tickets'}
        </button>
      )}
    </div>
  );
}
