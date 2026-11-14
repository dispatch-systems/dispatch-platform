import type { DspView } from '../../../core/accounts/api/index.js';
import { hasFeature } from '../../../core/shell/frontend/runtime/features.js';
import { readUpdateState } from '../../../core/shell/frontend/runtime/navigation-state.js';
import { dspHash, hashQuery } from '../../../core/shell/frontend/runtime/navigation.js';

// Each is a tab the platform switches on its own; the page has at least one while on.
const tabs = [
  ['timecards', 'Timecard', 'timecard.daily'],
  ['meal-breaks', 'Meal Breaks', 'timecard.meal_breaks'],
  ['employees', 'Employees', 'timecard.employees'],
] as const;

export const shownTimecardTabs = (view: DspView) =>
  tabs.filter(([, , feature]) => hasFeature(view, feature));
export const preferredTimecardTab = (
  shown: ReturnType<typeof shownTimecardTabs>,
  preferred: string | undefined,
) => shown.find(([id]) => id === preferred)?.[0] ?? shown[0]?.[0];

/**
 * The day a link opens Meal Breaks on with its Late DAs (`?late=`), as Home's Late DAs does:
 * Timecard opens that tab on that day, and Meal Breaks the Late DAs of it.
 */
export const lateLink = (query: URLSearchParams) => query.get('late') ?? undefined;

export function selectedTimecardTab(view: DspView) {
  return preferredTimecardTab(
    shownTimecardTabs(view),
    lateLink(hashQuery())
      ? 'meal-breaks'
      : readUpdateState<string | undefined>(
          'paycom-tab',
          undefined,
          dspHash(view.dsp.id, 'paycom'),
        ),
  );
}
