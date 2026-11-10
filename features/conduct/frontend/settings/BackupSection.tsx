import { useState } from 'react';
import { TriangleAlert } from 'lucide-react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import { saveSettings, useSettings } from '../../../../core/settings/api/client.js';
import { ApiError } from '../../../../core/shell/frontend/runtime/api.js';
import { offered } from '../../../../core/shell/frontend/runtime/exposes.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import { useConductBackup } from '../../api/client.js';

/** The section, and its one field, as the backend declares them. */
const SECTION = 'conduct.backup';
const FIELD = 'signed_copies';

/**
 * Whether signed copies are backed up where the DSP keeps its files, on Conduct's page of
 * Settings, only while something keeps them, named with where it keeps them. The switch shows
 * the choice at once and saves it; while the backup is failing, the section says so, and how
 * many copies wait.
 */
export function BackupSection({ view, refresh }: { view: DspView; refresh: () => void }) {
  const kept = offered('files', view);
  const { data: settings } = useSettings();
  const section = settings?.sections.find((each) => each.id === SECTION);
  // A choice shows over what was read until the section it was made on is replaced: by the
  // save's answer, or by Settings read again after another session saved first.
  const [chosen, setChosen] = useState<{ revision: number; on: boolean }>();
  const on =
    chosen && chosen.revision === section?.revision ? chosen.on : section?.values[FIELD] === true;
  const backup = useConductBackup(String(section?.revision));
  const save = useAction(async (next: boolean, revision: number) => {
    setChosen({ revision, on: next });
    try {
      await saveSettings(SECTION, { values: { [FIELD]: next }, revision });
    } catch (cause) {
      setChosen(undefined);
      if (cause instanceof ApiError && cause.code === 'settings_changed_reload_before_saving')
        refresh();
      throw cause;
    }
  });
  // Drawn once it can say where the copies go, so its wording never changes under the reader.
  const place = backup.data?.place;
  if (!kept || !section || !place) return null;
  const failing = on && backup.data?.failing ? backup.data : undefined;
  return (
    <section className="settings-section" aria-labelledby="conduct-backup">
      <h2 id="conduct-backup">Backup</h2>
      <label className="settings-field settings-toggle" htmlFor="conduct-backup-signed-copies">
        <span>
          Back up signed copies to {kept.name} in {place}
        </span>
        <input
          id="conduct-backup-signed-copies"
          type="checkbox"
          role="switch"
          checked={on}
          onChange={(event) => {
            if (!save.busy) void save.run(event.target.checked, section.revision);
          }}
        />
      </label>
      {failing && (
        <p className="conduct-backup-failing" role="status">
          <TriangleAlert size={16} aria-hidden />
          <span>
            {failing.why ?? `Couldn’t save to ${kept.name}`}
            {failing.waiting > 0 &&
              ` · ${failing.waiting} signed ${failing.waiting === 1 ? 'copy' : 'copies'} not backed up`}
          </span>
        </p>
      )}
    </section>
  );
}
