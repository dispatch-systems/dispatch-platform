import { useEffect } from 'react';
import type {
  CommunicationsWorkspace,
  Group,
  GroupEntry,
  GroupPerson,
} from '../../../api/index.js';
import { markGroupDone, messageGroup, slackAttachments } from '../../../api/client.js';
import { ErrorBox } from '../../../../../core/shell/frontend/ui/index.js';
import { useAction } from '../../../../../core/shell/frontend/runtime/useAction.js';
import { peopleText } from './groups.js';
import { Composer } from '../../conversations/Composer.js';
import { GroupFaces } from './GroupFaces.js';
import { GroupPeople } from './GroupPeople.js';
import { GroupThread } from './GroupThread.js';
import { PaneHead } from '../../shared/PaneHead.js';
import { ThreadPending } from '../../shared/ThreadFrame.js';
import { useGroup } from './useGroup.js';
import type { Changed } from '../../inbox/useInbox.js';
import { usePanel } from '../../shared/usePanel.js';

// A group open: its people's faces, its name, how many it went to, which opens who they are, and,
// while anything under it waits, how long and Done, for all of them; the DSP's messages to it and
// the answers; and writing to all of them again. On a phone it takes the page, with a way back.

export function GroupPane({
  id,
  listed,
  workspaces,
  docked,
  seen,
  changed,
  timeZone,
  now,
  show,
  back,
  closed,
  reply,
  open,
}: {
  id: string;
  /** The group as the list has it, which the page's own writes and each update keep. */
  listed?: Group;
  workspaces: readonly CommunicationsWorkspace[];
  /** Whether its people, open, are a column of their own beside it. */
  docked: boolean;
  /** Tells the page of the group as read, which pins one the list doesn't hold. */
  seen: (group: Group) => void;
  changed: Changed;
  timeZone: string;
  now: number;
  /** Shows the group in the list as the page's own writes left it. */
  show: (group: Group) => void;
  back: () => void;
  /** Leaves a group that's no longer there. */
  closed: () => void;
  /** Opens the conversation of whoever wrote an answer. */
  reply: (entry: GroupEntry) => void;
  /** Opens one of its people's conversation. */
  open: (person: GroupPerson) => void;
}) {
  const { opened, error, gone, readOlder, ...read } = useGroup(id, changed);
  useEffect(() => {
    if (gone) closed();
  }, [gone, closed]);
  useEffect(() => {
    if (opened) seen(opened.group);
  }, [opened, seen]);
  const group = listed ?? opened?.group;
  // Its people are written to where Communications works now.
  const writable = workspaces.some(
    (workspace) => workspace.on && group?.teamIds.includes(workspace.teamId),
  );
  const done = useAction(async () => {
    const after = await markGroupDone(id);
    read.show(after);
    show(after);
  });
  const people = usePanel();
  // Who they are is read again as the group is.
  const refresh = `${opened?.read ?? 0}`;
  return (
    <section
      className={people.open ? 'pane-thread profiled' : 'pane-thread'}
      aria-label={group?.name ?? 'Group'}
    >
      <PaneHead
        face={group && <GroupFaces faces={group.faces} size="head" />}
        name={group?.name}
        about={group && peopleText(group.people)}
        waits={group?.state === 'waiting'}
        waitingSince={group?.waitingSince ?? null}
        now={now}
        who={people.who}
        open={people.open}
        opens={people.opens}
        back={back}
        done={{ busy: done.busy, run: () => void done.run() }}
      />
      {people.drawn && group && (
        <GroupPeople
          id={people.id}
          group={group}
          refresh={refresh}
          timeZone={timeZone}
          now={now}
          docked={docked}
          leaving={!people.open}
          still={people.still}
          close={people.close}
          gone={people.gone}
          open={open}
        />
      )}
      <ErrorBox message={error} />
      {opened ? (
        <GroupThread
          entries={opened.entries}
          more={opened.more}
          readOlder={readOlder}
          timeZone={timeZone}
          now={now}
          reply={reply}
        />
      ) : (
        <ThreadPending failed={Boolean(error)} />
      )}
      {group && (
        <Composer
          attachments={slackAttachments}
          placeholder={`Message all ${group.people}`}
          writable={writable}
          send={async (text, photos) => {
            const entry = await messageGroup(id, text, photos);
            read.show(group, entry);
          }}
        />
      )}
    </section>
  );
}
