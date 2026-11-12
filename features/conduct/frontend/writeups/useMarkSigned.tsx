import { useState } from 'react';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { ConductOverview, ConductWriteupRow } from '../../api/index.js';
import { readWriteup } from '../../api/client.js';
import { SignDialog } from './SignDialog.js';

/** Marking a writeup signed from a list's row: the writeup read again, then its dialog. */
export function useMarkSigned(overview: ConductOverview, signed: () => void) {
  const [signing, setSigning] = useState<ConductWriteupRow>();
  const begin = useAction(async (row: ConductWriteupRow) => {
    const fresh = await readWriteup(row.id);
    if (!fresh.signedOn) setSigning(fresh);
  });
  return {
    markSigned: (row: ConductWriteupRow) => {
      if (!begin.busy) void begin.run(row);
    },
    dialog: signing && (
      <SignDialog
        overview={overview}
        writeup={signing}
        close={() => setSigning(undefined)}
        signed={() => {
          signed();
          setSigning(undefined);
        }}
      />
    ),
  };
}
