import type { ReactNode } from 'react';
import { Route } from 'lucide-react';
import { ErrorBox, Loading } from '../../../../core/shell/frontend/ui/index.js';
import { clock } from '../shared/format.js';
import type { RouteHeading } from './todays-routes.js';
import './headings.css';

// How Today's Routes names its groups, in the list and on the board, and what it shows as its read
// stands: loading, why it failed, or nobody on it.

/** What a group of rows is called: Waiting, its wave by when it starts, No wave, or No driver. */
export function headingText(heading: RouteHeading, timeZone: string) {
  switch (heading.kind) {
    case 'waiting':
      return 'Waiting';
    case 'wave':
      return clock(heading.at, timeZone);
    case 'no_wave':
      return 'No wave';
    case 'no_driver':
      return 'No driver';
  }
}

/** Today's Routes with nobody on it. */
export function NoRoutes() {
  return (
    <div className="no-routes">
      <Route aria-hidden="true" />
      No routes
    </div>
  );
}

/** How Today's Routes' read stands: being read for the first time, and why the latest failed. */
export type RoutesRead = { pending: boolean; error: string };

/** Today's Routes as its read stands, in the list or on the board: loading until it's first read,
 * why the latest read failed, and once read, its groups (`children`), or No routes. */
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
  if (read.pending) return <Loading />;
  return (
    <>
      <ErrorBox message={read.error} />
      {empty ? !read.error && <NoRoutes /> : children}
    </>
  );
}
