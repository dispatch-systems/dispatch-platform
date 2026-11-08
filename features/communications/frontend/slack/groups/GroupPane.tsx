import { useEffect, useRef } from 'react';
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
import type { Drafts } from '../../conversations/drafts.js';
import { GroupFaces } from './GroupFaces.js';
import { GroupPeople } from './GroupPeople.js';
import { GroupThread } from './GroupThread.js';
import { PaneHead } from '../../shared/PaneHead.js';
import { ThreadPending } from '../../shared/ThreadFrame.js';
import { useGroup } from './useGroup.js';
import type { Changed } from '../../inbox/useInbox.js';
import { usePanel } from '../../shared/usePanel.js';

// A group open: its people's faces, its name, how many it went to, which opens who they are, and,
// while anything under it waits, how long and Done, for all of them, else Next waiting; the DSP's
// messages to it and the answers; and writing to all of them again. On a phone it takes the page,
// with a way back.

export function GroupPane({
  id,
  listed,
  workspaces,
  docked,
  drafts,
  seen,
  changed,
  timeZone,
  now,
  show,
  back,
  closed,
  reply,
  open,
  next,
}: {
  id: string;
  /** The group as the list has it, which the page's own writes and each update keep. */
  listed?: Group;
  workspaces: readonly CommunicationsWorkspace[];
  /** Whether its people, open, are a column of their own beside it. */
  docked: boolean;
  /** What's written to each group, kept by the page. */
  drafts: Drafts;
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
  /** Next waiting: opens the one waiting longest of the section shown, while another waits. */
  next?: () => void;
}) {
  const { opened, error, gone, readOlder, olderFailed, retry, ...read } = useGroup(id, changed);
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
  // Done shows it done at once, and as the server had it once that's saved. One that fails takes
  // back only its own mark, and only while it still shows, keeping what newer updates brought.
  const latestShown = useRef(group);
  latestShown.current = group;
  const marking = useAction(async (seen: GroupEntry | null | undefined) => {
    const after = await markGroupDone(id, seen);
    read.show(after);
    show(after);
  });
  const done = () => {
    if (!group) return;
    const was = { state: group.state, waitingSince: group.waitingSince };
    const marked = { ...group, state: 'done' as const, waitingSince: null };
    read.show(marked);
    show(marked);
    void marking.run(opened?.entries.at(-1) ?? group.last).then((saved) => {
      const latest = latestShown.current;
      if (saved || latest?.state !== 'done' || latest.waitingSince !== null) return;
      read.show({ ...latest, ...was });
      show({ ...latest, ...was });
    });
  };
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
        done={done}
        next={next}
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
          olderFailed={olderFailed}
          timeZone={timeZone}
          now={now}
          reply={reply}
        />
      ) : (
        <ThreadPending failed={Boolean(error)} retry={retry} />
      )}
      {group && (
        <Composer
          drafts={drafts}
          draft={`group:${id}`}
          limits={opened?.limits}
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
