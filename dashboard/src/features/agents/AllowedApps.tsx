import { useState } from 'react';
import type { OAuthAllowedApp } from '../../../../shared/contracts/index.js';
import { allowOAuthApp, useOAuthApps } from '../../app/endpoints.js';
import { useAction } from '../../app/useAction.js';
import { DataState } from '../../ui/index.js';

/** The apps Dispatch lets sign in, each with its switch. Changing one asks for recent
 * verification, which the page's own prompt handles. */
export function AllowedApps() {
  const apps = useOAuthApps();
  // A change answers with the whole list, which stands until the list is next read.
  const [changed, setChanged] = useState<{ list: OAuthAllowedApp[]; over: unknown }>();
  const toggle = useAction(
    async (app: OAuthAllowedApp, allowed: boolean) => {
      const { apps: list } = await allowOAuthApp({ id: app.id, allowed });
      setChanged({ list, over: apps.data });
    },
    {
      success: (app, allowed) => `${app.name} ${allowed ? 'may connect' : 'may no longer connect'}`,
    },
  );
  const list = changed && changed.over === apps.data ? changed.list : apps.data?.apps;
  return (
    <DataState data={list} error={apps.error} retry={apps.refresh}>
      {(list) => (
        <ul className="agents-allowed">
          {list.map((app) => (
            <li key={app.id}>
              <label>
                <input
                  type="checkbox"
                  role="switch"
                  checked={app.allowed}
                  disabled={toggle.busy}
                  onChange={(event) => void toggle.run(app, event.target.checked)}
                />
                {app.name}
              </label>
            </li>
          ))}
        </ul>
      )}
    </DataState>
  );
}
