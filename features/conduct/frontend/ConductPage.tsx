import { useState } from 'react';
import { Plus } from 'lucide-react';
import type { DspView, SessionView } from '../../../core/accounts/api/index.js';
import { DataState, Header, Tabs } from '../../../core/shell/frontend/ui/index.js';
import type { ConductOverview, ConductSaved, ConductWriteupRow } from '../api/index.js';
import { useWriteup } from '../api/client.js';
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
}: {
  overview: ConductOverview;
  tab: 'drivers' | 'writeups';
  open: (address: ConductAddress, options?: { replace?: boolean }) => void;
  take: (overview: ConductOverview) => void;
}) {
  const [signing, setSigning] = useState<ConductWriteupRow>();
  return (
    <>
      <Header title="Conduct">
        <button type="button" className="primary" onClick={() => open({ view: 'write' })}>
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
          open({ view: 'list', tab: id === 'writeups' ? 'writeups' : 'drivers' }, { replace: true })
        }
      />
      {tab === 'drivers' ? (
        <DriversTab overview={overview} openDriver={(person) => open({ view: 'driver', person })} />
      ) : (
        <WriteupsTab
          overview={overview}
          openWriteup={(id) => open({ view: 'writeup', id })}
          markSigned={setSigning}
        />
      )}
      {signing && (
        <SignDialog
          overview={overview}
          writeup={signing}
          close={() => setSigning(undefined)}
          signed={(saved) => {
            take(saved.overview);
            setSigning(undefined);
          }}
        />
      )}
    </>
  );
}

/** Conduct: its drivers and writeups, one driver, one writeup, or one being written. */
export function ConductPage({ session, view }: { session: SessionView; view: DspView }) {
  const { overview, error, refresh, take } = useConduct();
  const { address, open, back } = useConductAddress();
  const supervisorName = `${session.user.firstName} ${session.user.lastName}`;
  return (
    <DataState data={overview} error={error} retry={refresh}>
      {(overview) => {
        const opened = (saved: ConductSaved) => {
          take(saved.overview);
          open({ view: 'writeup', id: saved.writeup.id }, { replace: true });
        };
        switch (address.view) {
          case 'list':
            return <Lists overview={overview} tab={address.tab} open={open} take={take} />;
          case 'driver':
            return (
              <DriverView
                overview={overview}
                code={address.person}
                back={back}
                write={() => open({ view: 'write', person: address.person })}
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
  );
}
