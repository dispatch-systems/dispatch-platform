import { useId, useState } from 'react';
import type { OAuthAllowedApp, OAuthAppId } from '../../../../shared/contracts/index.js';
import { allowOAuthApp, useOAuthApps } from '../../app/endpoints.js';
import { useAction } from '../../app/useAction.js';
import { DataState, Modal } from '../../ui/index.js';
import { AppIcon } from './AppIcon.js';

/** What the kinds of app that aren't one app are. */
const hints: Partial<Record<OAuthAppId, string>> = {
  local: 'Other apps that sign in from your computer',
  web: 'Apps that sign in from a website',
};

/** The apps Dispatch lets sign in, each with its switch, in a sheet. Changing one asks for
 * recent verification, which the page's own prompt handles. */
export function AllowedApps({ close }: { close: () => void }) {
  const apps = useOAuthApps();
  // A change answers with the whole list, which stands until the list is next read.
  const [changed, setChanged] = useState<{ list: OAuthAllowedApp[]; over: unknown }>();
  const toggle = useAction(
    async (app: OAuthAllowedApp, allowed: boolean) => {
      const { apps: list } = await allowOAuthApp({ id: app.id, allowed });
      setChanged({ list, over: apps.data });
    },
    {
      success: (app, allowed) =>
        allowed
          ? `${app.name} may make new connections`
          : `${app.name} may no longer make new connections; existing connections remain active`,
    },
  );
  const list = changed && changed.over === apps.data ? changed.list : apps.data?.apps;
  return (
    <Modal variant="sheet" title="Apps that may connect" onClose={close}>
      <div className="agents-allowed-sheet">
        <DataState data={list} error={apps.error} retry={apps.refresh}>
          {(list) => (
            <>
              <ul className="agents-allowed">
                {list.map((app) => (
                  <AllowedApp
                    key={app.id}
                    app={app}
                    busy={toggle.busy}
                    set={(allowed) => void toggle.run(app, allowed)}
                  />
                ))}
              </ul>
              <p className="agents-allowed-note">
                Turning an app off stops new connections. Apps already connected keep working until
                you revoke them.
              </p>
            </>
          )}
        </DataState>
      </div>
    </Modal>
  );
}

function AllowedApp({
  app,
  busy,
  set,
}: {
  app: OAuthAllowedApp;
  busy: boolean;
  set: (allowed: boolean) => void;
}) {
  const id = useId();
  const hint = hints[app.id];
  return (
    <li>
      <label>
        <span className="agents-allowed-app">
          <AppIcon app={app.id} />
          <span>
            <span id={`${id}-name`}>{app.name}</span>
            {hint && <small id={`${id}-hint`}>{hint}</small>}
          </span>
        </span>
        <input
          type="checkbox"
          role="switch"
          aria-labelledby={`${id}-name`}
          aria-describedby={hint ? `${id}-hint` : undefined}
          checked={app.allowed}
          disabled={busy}
          onChange={(event) => set(event.target.checked)}
        />
      </label>
    </li>
  );
}
