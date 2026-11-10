import type { ReactNode } from 'react';
import { Route } from 'lucide-react';
import type { TodaysRoutes } from '../../api/index.js';
import { DataState } from '../../../../core/shell/frontend/ui/index.js';
import type { RouteGroup } from './todays-routes.js';
import './headings.css';

// How Today's Routes names its groups, in the list and on the board, and what it shows as its read
// stands: loading, paused or offline, why it failed, or nobody on it.

/** What a group of rows is called: the drivers nothing, the work phones nobody held No driver. */
export const groupHeading = (group: RouteGroup) =>
  group.key === 'no_driver' ? 'No driver' : undefined;

/** Today's Routes with nobody on it. */
function NoRoutes() {
  return (
    <div className="no-routes">
      <Route aria-hidden="true" />
      No routes
    </div>
  );
}

/** How Today's Routes' read stands: what it last answered, if it has, why the latest failed, and
 * reading it again. */
export type RoutesRead = { routes?: TodaysRoutes; error: string; retry: () => void };

/** Today's Routes as its read stands, in the list or on the board, by the dashboard's one loading
 * rule (`DataState`): until a read answers, loading, or why it can't read now, or why it failed;
 * once one has, its groups (`children`), or No routes, beside why the latest read failed. */
export function RoutesState({
  read,
  empty,
  children,
}: {
  read: RoutesRead;
  /** Whether it holds nobody. */
  empty: boolean;
  children: ReactNode;
}) {
  return (
    <DataState data={read.routes} error={read.error} retry={read.retry}>
      {() => (empty ? <NoRoutes /> : children)}
    </DataState>
  );
}
