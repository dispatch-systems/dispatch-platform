import { useCallback, useEffect } from 'react';
import { useData } from '../../../../../core/shell/frontend/runtime/api.js';
import { call, channelLinking, workspaceUrl } from '../../../api/client.js';
import type { SlackCanvases } from '../../../api/generated/SlackCanvases.js';
import type { SlackChannelCanvases } from '../../../api/generated/SlackChannelCanvases.js';

// Sync Canvases's endpoints, as its section and its card in a workspace's settings call them.

/** Sync Canvases in the workspace `team`: where documents go, the channels linked, and each canvas kept. */
const canvasReaders = new Set<() => void>();
export function useSlackCanvases(team: string) {
  const url = `${workspaceUrl(team)}/canvases`;
  const read = useCallback(
    (signal: AbortSignal) => call<SlackCanvases>(url, undefined, signal),
    [url],
  );
  // Canvases are written as Slack tells of them, so an open section reads them again every 10 seconds.
  const canvases = useData<SlackCanvases>(url, 10000, undefined, undefined, false, read);
  const { refresh } = canvases;
  useEffect(() => {
    canvasReaders.add(refresh);
    return () => void canvasReaders.delete(refresh);
  }, [refresh]);
  return canvases;
}
/** Has every reading of Sync Canvases read it again, after a change. */
const canvasesChanged = <T>(answer: T) => {
  for (const read of canvasReaders) read();
  return answer;
};
/** Sync Canvases' channels: every canvas in each kept in its folder. */
export const canvasLinking = channelLinking<SlackCanvases>('canvases', canvasesChanged);
/** The canvases in the workspace `team`'s channel `channel`, to choose from. */
export function useSlackChannelCanvases(team: string, channel: string) {
  const url = `${workspaceUrl(team)}/canvases/channels/${encodeURIComponent(channel)}`;
  const read = useCallback(
    (signal: AbortSignal) => call<SlackChannelCanvases>(url, undefined, signal),
    [url],
  );
  return useData<SlackChannelCanvases>(url, 0, undefined, undefined, false, read);
}
/** Chooses the canvases `canvases` of the channel `channel`, for the folder `folder` or a new one named after it. */
export const chooseSlackCanvases = (
  team: string,
  channel: string,
  canvases: readonly string[],
  folder?: string,
) =>
  call<SlackCanvases>(`${workspaceUrl(team)}/canvases/chosen`, { channel, canvases, folder }).then(
    canvasesChanged,
  );
/** Stops syncing the canvas `canvas`, chosen on its own. */
export const unchooseSlackCanvas = (team: string, canvas: string) =>
  call<SlackCanvases>(
    `${workspaceUrl(team)}/canvases/chosen/${encodeURIComponent(canvas)}/remove`,
    {},
  ).then(canvasesChanged);
