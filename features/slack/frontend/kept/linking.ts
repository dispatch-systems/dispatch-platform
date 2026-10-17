import type { SlackFolder } from '../../api/generated/SlackFolder.js';
import type { SlackFolders } from '../../api/generated/SlackFolders.js';

// One of Slack's integrations that links channels to folders where the DSP keeps its files, as
// the shared pieces of its section call on it: Import Files, and Sync Canvases.

/** A channel linked to a folder, as a section lists it. */
export interface LinkedChannel {
  channelId: string;
  channelName: string;
  private: boolean;
  folder: SlackFolder;
  /** Whether the folder is gone, so what comes can't be kept until another is chosen. */
  folderMissing: boolean;
}

export interface Linking {
  /** Where the folder `folder`, or the top of where files are kept, is browsed. */
  foldersUrl: (team: string, folder?: string) => string;
  browse: (team: string, folder?: string, signal?: AbortSignal) => Promise<SlackFolders>;
  /** Links the channel `channel` to the folder `folder`, or to a new one named after it. */
  link: (team: string, channel: string, folder?: string) => Promise<unknown>;
  /** Keeps the linked channel `channel`'s things in the folder `folder` from now on. */
  move: (team: string, channel: string, folder: string) => Promise<unknown>;
  unlink: (team: string, channel: string) => Promise<unknown>;
}
