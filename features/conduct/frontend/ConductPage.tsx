import { useCallback, useEffect, useState } from 'react';
import { Plus } from 'lucide-react';
import type { DspView, SessionView } from '../../../core/accounts/api/index.js';
import { DataState, ErrorBox, Header, Tabs } from '../../../core/shell/frontend/ui/index.js';
import type {
  ConductOverview,
  ConductSaved,
  ConductWriteup,
  ConductWriteupRow,
} from '../api/index.js';
import { useAction } from '../../../core/shell/frontend/runtime/useAction.js';
import { LeaveGuards, type Guards } from '../../../core/shell/frontend/runtime/leave.js';
import { usePrintNotice } from './notice/usePrintNotice.js';
import { type HistoryFilters, useWriteup } from '../api/client.js';
import { useConductAddress, type ConductAddress } from './address.js';
import { WriteupForm } from './compose/WriteupForm.js';
import { draftOf, emptyDraft } from './compose/draft.js';
import { DriverView } from './drivers/DriverView.js';
import { DriversTab } from './drivers/DriversTab.js';
import { useConduct } from './useConduct.js';
import { SignDialog } from './writeups/SignDialog.js';
import { WriteupView } from './writeups/WriteupView.js';
import { WriteupsTab } from './writeups/WriteupsTab.js';

/** Changing a writeup not yet signed: the form, once the writeup is read. */
function EditWriteup({
  id,
  ...form
}: { id: string } & Omit<Parameters<typeof WriteupForm>[0], 'initial' | 'editing'>) {
  const read = useWriteup(id);
  return (
    <DataState data={read.data} error={read.error} retry={read.refresh}>
      {(writeup) => <WriteupForm {...form} initial={draftOf(writeup)} editing={writeup} />}
    </DataState>
  );
}

/** The drivers and the writeups, in tabs, with a writeup marked signed from its row. */
function Lists({
  overview,
  tab,
  open,
  take,
  filters,
  change,
  write,
}: {
  overview: ConductOverview;
  tab: 'drivers' | 'writeups';
  open: (address: ConductAddress, options?: { replace?: boolean }) => void;
  take: () => void;
  filters: HistoryFilters;
  change: (filters: HistoryFilters) => void;
  write: () => void;
}) {
  const [signing, setSigning] = useState<ConductWriteupRow>();
  return (
    <>
      <Header title="Conduct">
        <button type="button" className="primary" onClick={write}>
          <Plus size={16} />
          New writeup
        </button>
      </Header>
      <Tabs
        label="Conduct"
        value={tab}
        items={[
          ['drivers', 'Drivers'],
          ['writeups', 'Writeups'],
        ]}
        onChange={(id) =>
          open(
            { view: 'list', tab: id === 'writeups' ? 'writeups' : 'drivers', filters },
            { replace: true },
          )
        }
      />
      {tab === 'drivers' ? (
        <DriversTab
          overview={overview}
          filters={filters}
          change={change}
          openDriver={(person) => open({ view: 'driver', person })}
        />
      ) : (
        <WriteupsTab
          overview={overview}
          filters={filters}
          change={change}
          openWriteup={(id) => open({ view: 'writeup', id })}
          markSigned={setSigning}
        />
      )}
      {signing && (
        <SignDialog
          overview={overview}
          writeup={signing}
          close={() => setSigning(undefined)}
          signed={() => {
            take();
            setSigning(undefined);
          }}
        />
      )}
    </>
  );
}

/** Conduct: its drivers and writeups, one driver, one writeup, or one being written. */
export function ConductPage({ session, view }: { session: SessionView; view: DspView }) {
  const { overview, error, refresh, take, reload } = useConduct(view.dsp.timezone);
  const [guards] = useState<Guards>(() => new Set());
  const { address, open, back, filters } = useConductAddress(guards);
  const [toPrint, setToPrint] = useState<ConductWriteup>();
  const clearPrint = useCallback(() => setToPrint(undefined), []);
  const start = useAction(async (person?: string) => {
    await reload();
    open({ view: 'write', person });
  });
  const supervisorName = `${session.user.firstName} ${session.user.lastName}`;
  return (
    <LeaveGuards value={guards}>
      <DataState data={overview} error={error} retry={refresh}>
        {(overview) => {
          const opened = (saved: ConductSaved, andPrint = false) => {
            take();
            open({ view: 'writeup', id: saved.writeup.id }, { replace: true, saved: true });
            if (andPrint) setToPrint(saved.writeup);
          };
          switch (address.view) {
            case 'list':
              return (
                <Lists
                  overview={overview}
                  tab={address.tab}
                  filters={filters}
                  change={(filters) => open({ ...address, filters }, { replace: true })}
                  open={open}
                  take={() => take()}
                  write={() => {
                    void start.run();
                  }}
                />
              );
            case 'driver':
              return (
                <DriverView
                  key={address.person}
                  overview={overview}
                  code={address.person}
                  back={back}
                  write={() => {
                    void start.run(address.person);
                  }}
                  openWriteup={(id) => open({ view: 'writeup', id })}
                />
              );
            case 'writeup':
              return (
                <WriteupView
                  key={address.id}
                  overview={overview}
                  id={address.id}
                  back={back}
                  edit={() => open({ view: 'write', edit: address.id })}
                  take={take}
                />
              );
            case 'write':
              return address.edit ? (
                <EditWriteup
                  key={address.edit}
                  id={address.edit}
                  overview={overview}
                  supervisorName={supervisorName}
                  cancel={back}
                  saved={opened}
                />
              ) : (
                <WriteupForm
                  key={view.token}
                  overview={overview}
                  initial={emptyDraft(overview.today, address.person)}
                  supervisorName={supervisorName}
                  cancel={back}
                  saved={opened}
                />
              );
          }
        }}
      </DataState>
      {toPrint && address.view === 'writeup' && address.id === toPrint.id && (
        <SavedPrint key={toPrint.id} writeup={toPrint} closed={clearPrint} />
      )}
    </LeaveGuards>
  );
}

function SavedPrint({ writeup, closed }: { writeup: ConductWriteup; closed: () => void }) {
  useEffect(() => () => closed(), [closed]);
  const printing = usePrintNotice();
  const [started, setStarted] = useState(false);
  useEffect(() => {
    if (!started) {
      setStarted(true);
      printing.print(writeup);
    }
  }, [started, printing, writeup]);
  return (
    <>
      <ErrorBox message={printing.error} />
      {printing.sheet}
    </>
  );
}
