import type { DspView, SessionView } from '../../../core/accounts/api/index.js';
import type { SettingsAnswer } from '../../../core/settings/api/index.js';
import { useSettings } from '../../../core/settings/api/client.js';
import { DataState, Header } from '../../../core/shell/frontend/ui/index.js';
import { guards } from './address.js';
import { useFound } from './find.js';
import { SettingsSections } from './SettingsSections.js';
import { useOpenPage } from './useOpenPage.js';
import './settings.css';

/**
 * A DSP's Settings: the page of it the address names, under its name. Its sidebar, drawn in
 * place of the app's, lists the member's pages and searches them.
 */
export function SettingsPage({ session, view }: { session: SessionView; view: DspView }) {
  const settings = useSettings();
  return (
    <DataState data={settings.data} error={settings.error} retry={settings.refresh}>
      {(data) => (
        // Another DSP, or another role viewed as, starts afresh: no page, edit, editor or
        // section pointed out of the last one's carries over.
        <Page
          key={view.token}
          session={session}
          view={view}
          settings={data}
          refresh={settings.refresh}
        />
      )}
    </DataState>
  );
}

function Page({
  session,
  view,
  settings,
  refresh,
}: {
  session: SessionView;
  view: DspView;
  settings: SettingsAnswer;
  refresh: () => void;
}) {
  const open = useOpenPage(settings, view);
  const found = useFound();
  return (
    <>
      <Header title={open.pages.find((page) => page.id === open.page)?.label ?? 'Settings'} />
      <SettingsSections
        session={session}
        view={view}
        settings={settings}
        page={open.page}
        refresh={refresh}
        guards={guards}
        busy={open.pending}
        found={found}
      />
    </>
  );
}
