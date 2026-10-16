import { useCallback, useEffect } from 'react';
import { useData, wordedApi } from '../../../core/shell/frontend/runtime/api.js';
import type { SlackAccount } from './generated/SlackAccount.js';
import type { SlackChannel } from './generated/SlackChannel.js';
import type { SlackChannels } from './generated/SlackChannels.js';
import type { SlackInstall } from './generated/SlackInstall.js';
import type { SlackNewLink } from './generated/SlackNewLink.js';

export type { SlackAccount } from './generated/SlackAccount.js';
export type { SlackChannel } from './generated/SlackChannel.js';
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
};
const call = wordedApi(wording);

/** The DSP's workspaces and install link, as those who manage its connections see them. */
export const slackAccountUrl = '/api/dsp/slack/account';
// A reader stays the same function from one render to the next: a new one reads again.
const readAccount = (signal: AbortSignal) => call<SlackAccount>(slackAccountUrl, undefined, signal);
/** How each reading of the account reads it again, when another changed it. */
const readers = new Set<() => void>();
export function useSlackAccount() {
  const account = useData<SlackAccount>(
    slackAccountUrl,
    0,
    undefined,
    undefined,
    false,
    readAccount,
  );
  const { refresh } = account;
  useEffect(() => {
    readers.add(refresh);
    return () => void readers.delete(refresh);
  }, [refresh]);
  return account;
}
/** Has every reading of the account read it again, after a change: New's, shown on the cards. */
export const slackAccountChanged = () => {
  for (const read of readers) read();
};
/**
 * Starts installing Dispatch in a workspace: a new one, to have the name `label` in Dispatch,
 * or `team` to reconnect or update one of the DSP's. Where the browser goes next.
 */
export const connectSlack = (asked: { team: string } | { label: string }) =>
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
const workspaceUrl = (team: string) => `/api/dsp/slack/workspaces/${encodeURIComponent(team)}`;
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
 * A new install link, in place of any the DSP had, for a workspace to have the name `label` in
 * Dispatch: shown this once.
 */
export const createSlackLink = (label: string) =>
  call<SlackNewLink>('/api/dsp/slack/link', { label });
/** Ends the DSP's install link. */
export const cancelSlackLink = () => call<SlackAccount>('/api/dsp/slack/link/cancel', {});
