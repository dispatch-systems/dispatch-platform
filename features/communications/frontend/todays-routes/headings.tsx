import { Route } from 'lucide-react';
import { clock } from '../shared/format.js';
import type { RouteHeading } from './todays-routes.js';

// How Today's Routes names its groups, in the list and on the board, and what it shows with
// nobody on it.

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
