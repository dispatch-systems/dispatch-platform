import { useCallback, useState } from 'react';
import { useCollectionUpdates } from '../../../../core/collection/frontend/live-collection.js';
import { dateFormatter } from '../../../../core/shell/frontend/lib/date-format.js';
import { DataState } from '../../../../core/shell/frontend/ui/index.js';
import { useDriverMatch } from '../../api/client.js';
import { DriverSheet } from '../person/DriverSheet.js';
import { VariantList } from './VariantList.js';
import { PeopleTable } from './PeopleTable.js';
import { ReviewList } from './ReviewList.js';
import { RosterBar } from './RosterBar.js';

/** Settings → Driver Match: who may be listed twice, name variants to confirm, then everyone. */
export function DriverMatchSettings({
  timezone,
  canCollect,
}: {
  timezone: string;
  canCollect: boolean;
}) {
  // A finished roster collection brings its people in.
  useCollectionUpdates();
  const request = useDriverMatch();
  const [open, setOpen] = useState<string | null>(null);
  const onOpen = useCallback((code: string) => setOpen(code), []);
  const today = dateFormatter('en-CA', { timeZone: timezone }).format(new Date());
  return (
    <div className="driver-match">
      <DataState data={request.data} error={request.error}>
        {(data) => (
          <>
            <RosterBar counts={data.counts} timezone={timezone} canCollect={canCollect} />
            <ReviewList pairs={data.review} drivers={data.drivers} />
            <VariantList drivers={data.drivers} onOpen={onOpen} />
            <PeopleTable data={data} today={today} timezone={timezone} onOpen={onOpen} />
            {open && (
              <DriverSheet
                code={open}
                drivers={data.drivers}
                timezone={timezone}
                today={today}
                onClose={() => setOpen(null)}
              />
            )}
          </>
        )}
      </DataState>
    </div>
  );
}
