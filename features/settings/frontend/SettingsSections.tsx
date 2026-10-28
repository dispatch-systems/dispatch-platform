import { Suspense } from 'react';
import type { DspView, SessionView } from '../../../core/accounts/api/index.js';
import type { SettingsAnswer } from '../../../core/settings/api/index.js';
import { LeaveGuards, type Guards } from '../../../core/shell/frontend/runtime/leave.js';
import { settingsSection } from '../../../core/shell/frontend/runtime/slots.js';
import { Loading } from '../../../core/shell/frontend/ui/index.js';
import { whoseSettings } from './drafts.js';
import type { Found } from './find.js';
import { Section, sectionElement } from './Section.js';
import { drawnSections, sectionsOn } from './pages.js';

/**
 * One page of Settings: its sections in their order, a declared one drawn from what the server
 * says of it and a custom one by its owner's component, which draws those that go with a
 * collection, as the Collectors page does. Each section's element is
 * `#settings-section-<id>`, marked `data-settings-section="<id>"`; the one `found` is pointed
 * out. `guards` are what the page asks before it moves to another; `busy` while it is moving.
 */
export function SettingsSections({
  session,
  view,
  settings,
  page,
  refresh,
  guards,
  busy = false,
  found,
}: {
  session: SessionView;
  view: DspView;
  settings: SettingsAnswer;
  page: string | undefined;
  refresh: () => void;
  guards: Guards;
  busy?: boolean;
  found?: Found;
}) {
  const reveal = (id: string) => (found?.section === id ? found.reveal : undefined);
  const sections = sectionsOn(settings, view, page);
  const label = settings.pages.find((each) => each.id === page)?.label;
  const whose = whoseSettings(session, view);
  const withCollections = drawnSections(settings, view).filter(
    (section) => section.page === page && section.collection,
  );
  const collectionSections = (schedule: string) =>
    withCollections
      .filter((section) => section.collection === schedule)
      .map((section) => (
        <Section
          key={section.id}
          section={section}
          whose={whose}
          timezone={view.dsp.timezone}
          refresh={refresh}
          reveal={reveal(section.id)}
        />
      ));
  const pointed =
    withCollections.find((section) => section.id === found?.section)?.collection ?? undefined;
  return (
    <LeaveGuards value={guards}>
      <div className="settings-page" aria-busy={busy || undefined} inert={busy}>
        <Suspense fallback={<Loading />}>
          {sections.map((section) =>
            section.custom ? (
              <div key={section.id} {...sectionElement(section.id, reveal(section.id))}>
                {settingsSection(section.id)?.render({
                  session,
                  view,
                  refresh,
                  collectionSections,
                  pointed,
                })}
              </div>
            ) : (
              <Section
                key={section.id}
                section={section}
                whose={whose}
                timezone={view.dsp.timezone}
                refresh={refresh}
                reveal={reveal(section.id)}
                named={sections.length === 1 && section.label === label}
              />
            ),
          )}
        </Suspense>
      </div>
    </LeaveGuards>
  );
}
