import { useCallback, useEffect, useMemo, useSyncExternalStore } from 'react';
import { wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import { useData } from '../../../core/shell/frontend/runtime/data-reads.js';
import type { Linking } from '../frontend/linked/linking.js';
import type { SlackAccount } from './generated/SlackAccount.js';
import type { SlackChannel } from './generated/SlackChannel.js';
import type { SlackChannels } from './generated/SlackChannels.js';
import type { SlackFolders } from './generated/SlackFolders.js';
import type { SlackInstall } from './generated/SlackInstall.js';
import type { SlackNewLink } from './generated/SlackNewLink.js';

export type { SlackAccount } from './generated/SlackAccount.js';
export type { SlackCanvas } from './generated/SlackCanvas.js';
export type { SlackCanvases } from './generated/SlackCanvases.js';
export type { SlackCanvasLink } from './generated/SlackCanvasLink.js';
export type { SlackChannel } from './generated/SlackChannel.js';
export type { SlackChannelCanvas } from './generated/SlackChannelCanvas.js';
export type { SlackChannelCanvases } from './generated/SlackChannelCanvases.js';
export type { SlackFolder } from './generated/SlackFolder.js';
export type { SlackFolders } from './generated/SlackFolders.js';
export type { SlackImportedFile } from './generated/SlackImportedFile.js';
export type { SlackImportLink } from './generated/SlackImportLink.js';
export type { SlackImports } from './generated/SlackImports.js';
export type { SlackWorkspace } from './generated/SlackWorkspace.js';
export type { SlackLink } from './generated/SlackLink.js';
export type { SlackNewLink } from './generated/SlackNewLink.js';

// Slack's endpoints, as its cards on Settings' Connections, New there and its workspaces'
// settings call them.

/** What its error codes say. Only its cards, New and its settings make the calls that raise them. */
const wording: Record<string, string> = {
  slack_unavailable: "Slack isn't set up for Dispatch yet.",
  slack_unreachable: "Slack didn't answer. Try again in a minute.",
  slack_rate_limited: 'Slack is busy. Try again in a minute.',
  slack_install_failed: "Slack didn't finish. Try again.",
  slack_refused: "Slack didn't finish. Try again.",
  slack_connect_expired: 'That took too long or was already used. Connect again.',
  slack_workspace_taken: 'That workspace is connected to another DSP.',
  slack_workspace_connected: 'That workspace is already connected.',
  slack_workspace_not_found: "That workspace isn't connected. Refresh the page.",
  slack_organization_install: 'Add Dispatch to one workspace at a time.',
  slack_connection_broken: 'Slack stopped accepting the connection. Reconnect the workspace.',
  slack_needs_update: 'This workspace needs updating first.',
  slack_integration_not_found: "That integration isn't available. Refresh the page.",
  slack_import_off: 'Import Files is off in this workspace.',
  slack_canvases_off: 'Sync Canvases is off in this workspace.',
  slack_canvas_not_found: "That canvas isn't in the channel anymore. Refresh the list.",
  slack_canvas_not_chosen: "That canvas isn't synced. Refresh the page.",
  slack_files_unavailable: "There's nowhere to save files. Refresh the page.",
  slack_files_forbidden: "You don't have access to where files are saved.",
  slack_files_unready: "Files can't be saved yet.",
  slack_channel_not_joined: 'Join the channel first.',
  slack_channel_linked: 'That channel is already linked.',
  slack_channel_not_linked: "That channel isn't linked. Refresh the page.",
  files_folder_not_found: "That folder isn't there anymore. Choose another.",
};
/** Its calls, worded; its integrations' calls in their folders make them too. */
export const call = wordedApi(wording);

/** The DSP's workspaces and install link, as those who manage its connections see them. */
export const slackAccountUrl = '/api/dsp/slack/account';
// A reader stays the same function from one render to the next: a new one reads again.
const readAccount = (signal: AbortSignal) => call<SlackAccount>(slackAccountUrl, undefined, signal);
/** How each reading of the account reads it again, when another changed it. */
const readers = new Set<() => void>();
// Integrations switched here and not yet read as switched, by workspace and integration: every
// reading of the account shows them at once. Each goes once a reading has it as switched, or
// once its switch fails.
const switching = new Map<string, Map<string, boolean>>();
const watchers = new Set<() => void>();
let switched = 0;
const switchingChanged = () => {
  switched += 1;
  for (const watch of watchers) watch();
};
const watchSwitching = (watch: () => void) => {
  watchers.add(watch);
  return () => void watchers.delete(watch);
};
export function useSlackAccount() {
  const account = useData<SlackAccount>(
    slackAccountUrl,
    0,
    undefined,
    undefined,
    false,
    readAccount,
  );
  const { refresh, data: read } = account;
  useEffect(() => {
    readers.add(refresh);
    return () => void readers.delete(refresh);
  }, [refresh]);
  const version = useSyncExternalStore(watchSwitching, () => switched);
  // A reading that has a switch as it was made retires it.
  useEffect(() => {
    let retired = false;
    for (const workspace of read?.workspaces ?? [])
      for (const [id, on] of switching.get(workspace.teamId) ?? [])
        if (workspace.integrations.includes(id) === on) {
          switching.get(workspace.teamId)!.delete(id);
          retired = true;
        }
    if (retired) switchingChanged();
  }, [read]);
  const data = useMemo(
    () =>
      read && {
        ...read,
        workspaces: read.workspaces.map((workspace) => {
          const asked = switching.get(workspace.teamId);
          if (!asked?.size) return workspace;
          const integrations = workspace.integrations.filter((id) => asked.get(id) !== false);
          for (const [id, on] of asked) if (on && !integrations.includes(id)) integrations.push(id);
          return { ...workspace, integrations };
        }),
      },
    // A switch made or retired shows in the next reading of it.
    [read, version],
  );
  return { ...account, data };
}
/** Has every reading of the account read it again, after a change: New's, shown on the cards. */
export const slackAccountChanged = () => {
  for (const read of readers) read();
};
/**
 * Starts installing Dispatch in a workspace: a new one, to have the name `label` in Dispatch,
 * or `team` to reconnect or update one of the DSP's. Slack sends the browser back to the DSP's
 * page `page`, which finishes it with `useSlackReturn`, or else to Settings' Connections. Where
 * the browser goes next.
 */
export const connectSlack = (asked: ({ team: string } | { label: string }) & { page?: string }) =>
  call<SlackInstall>('/api/dsp/slack/connect', asked);
/** Finishes the install Slack sent the browser back from. */
export const finishSlack = (state: string, code: string) =>
  call<SlackAccount>('/api/dsp/slack/connect/finish', { state, code });
/** Names the workspace `team` in Dispatch, or with a blank `label`, has it go by its name in Slack. */
export const renameSlack = (team: string, label: string) =>
  call<SlackAccount>(`/api/dsp/slack/workspaces/${encodeURIComponent(team)}/rename`, { label });
/** Takes Dispatch out of the workspace `team`. */
export const disconnectSlack = (team: string) =>
  call<SlackAccount>(`/api/dsp/slack/workspaces/${encodeURIComponent(team)}/disconnect`, {});
export const workspaceUrl = (team: string) =>
  `/api/dsp/slack/workspaces/${encodeURIComponent(team)}`;
/** The address of the workspace `team`'s logo, at its `version`, which it carries. */
export const workspaceLogoUrl = (team: string, version: string) =>
  `${workspaceUrl(team)}/logo?v=${version}`;
/** The channels of the workspace `team` Dispatch may be in, with whether it's in each. */
export function useSlackChannels(team: string) {
  const url = `${workspaceUrl(team)}/channels`;
  const read = useCallback(
    (signal: AbortSignal) => call<SlackChannels>(url, undefined, signal),
    [url],
  );
  return useData<SlackChannels>(url, 0, undefined, undefined, false, read);
}
/** Has Dispatch join the workspace `team`'s channel `channel`, or leave it: the channel after. */
export const setSlackChannel = (team: string, channel: string, member: boolean) =>
  call<SlackChannel>(
    `${workspaceUrl(team)}/channels/${encodeURIComponent(channel)}/${member ? 'join' : 'leave'}`,
    {},
  );
/**
 * Switches the integration `id` on or off in the workspace `team`: every reading of the account
 * shows it switched at once, and reads the account again once it is; one that fails is taken back.
 */
export async function switchSlackIntegration(team: string, id: string, on: boolean) {
  const asked = switching.get(team) ?? new Map<string, boolean>();
  switching.set(team, asked.set(id, on));
  switchingChanged();
  try {
    await call<SlackAccount>(`${workspaceUrl(team)}/integrations/${encodeURIComponent(id)}`, {
      on,
    });
  } catch (cause) {
    asked.delete(id);
    switchingChanged();
    throw cause;
  }
  slackAccountChanged();
}
/**
 * How the integration whose endpoints are under `path` links channels to folders: browsing the
 * folders, and linking, moving and unlinking a channel, each change read again by `changed`.
 */
export const channelLinking = <T>(path: string, changed: (answer: T) => T): Linking => {
  const foldersUrl = (team: string, folder?: string) =>
    `${workspaceUrl(team)}/${path}/folders${folder ? `?folder=${encodeURIComponent(folder)}` : ''}`;
  const links = (team: string) => `${workspaceUrl(team)}/${path}/links`;
  return {
    foldersUrl,
    browse: (team, folder, signal) =>
      call<SlackFolders>(foldersUrl(team, folder), undefined, signal),
    link: (team, channel, folder) => call<T>(links(team), { channel, folder }).then(changed),
    move: (team, channel, folder) =>
      call<T>(`${links(team)}/${encodeURIComponent(channel)}/folder`, { folder }).then(changed),
    unlink: (team, channel) =>
      call<T>(`${links(team)}/${encodeURIComponent(channel)}/remove`, {}).then(changed),
  };
};
/**
 * A new install link for adding a workspace, in place of the one the DSP had, for it to have the
 * name `label` in Dispatch: shown this once.
 */
export const createSlackLink = (label: string) =>
  call<SlackNewLink>('/api/dsp/slack/link', { label });
/**
 * A new install link for updating or reconnecting the workspace `team`, in place only of the
 * one it had: shown this once.
 */
export const createWorkspaceLink = (team: string) =>
  call<SlackNewLink>(`${workspaceUrl(team)}/link`, {});
/** Ends the DSP's install link for adding a workspace. */
export const cancelSlackLink = () => call<SlackAccount>('/api/dsp/slack/link/cancel', {});
