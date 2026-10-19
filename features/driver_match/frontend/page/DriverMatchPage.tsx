import { useCallback, useMemo, useState } from 'react';
import { useCollectionUpdates } from '../../../../core/collection/frontend/live-collection.js';
import { localDate } from '../../../../core/shell/frontend/lib/day.js';
import { DataState, Header } from '../../../../core/shell/frontend/ui/index.js';
import type { DriverMatch } from '../../api/index.js';
import { useDriverMatch } from '../../api/client.js';
import { PersonSheet } from '../person/PersonSheet.js';
import { CheckBanner } from './CheckBanner.js';
import { CheckDialog } from './CheckDialog.js';
import { checkList } from './checks.js';
import { PeopleGrid } from './PeopleGrid.js';

/** Driver Match's page: who to check, then everyone by face. */
export function DriverMatchPage({ timezone }: { timezone: string }) {
  // A finished collection can bring new people in.
  useCollectionUpdates();
  const request = useDriverMatch();
  return (
    <div className="driver-match">
      <Header title="Driver Match" />
      <DataState data={request.data} error={request.error}>
        {(data) => <People data={data} timezone={timezone} />}
      </DataState>
    </div>
  );
}

function People({ data, timezone }: { data: DriverMatch; timezone: string }) {
  const checks = useMemo(() => checkList(data.questions, data.people), [data]);
  const queued = useMemo(() => new Set(checks.flatMap((check) => check.codes)), [checks]);
  const [open, setOpen] = useState<string | null>(null);
  // The question the dialog opens on, by its place; none while it is closed.
  const [checking, setChecking] = useState<number | null>(null);
  // Only a person a question waits on has a card that opens one.
  const onCheck = useCallback(
    (code: string) => setChecking(checks.findIndex((check) => check.codes.includes(code))),
    [checks],
  );
  const onClose = useCallback(() => setChecking(null), []);
  const today = localDate(timezone);
  return (
    <>
      <CheckBanner checks={checks} onStart={() => setChecking(0)} />
      <PeopleGrid
        people={data.people}
        sources={data.sources}
        queued={queued}
        onOpen={setOpen}
        onCheck={onCheck}
      />
      {checking !== null && <CheckDialog checks={checks} start={checking} onClose={onClose} />}
      {open && (
        <PersonSheet
          code={open}
          people={data.people}
          timezone={timezone}
          today={today}
          onClose={() => setOpen(null)}
        />
      )}
    </>
  );
}
