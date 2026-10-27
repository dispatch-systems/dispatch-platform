import { useId, useMemo, useRef } from 'react';
import { slackAttachments } from '../../api/client.js';
import { Composer } from '../conversations/Composer.js';
import { ListPane } from '../page/ListPane.js';
import { roleText } from '../shared/format.js';
import { PartyFace } from '../conversations/party.js';
import { PaneHead } from '../shared/PaneHead.js';
import { ProfileShown } from '../conversations/Profile.js';
import { Thread } from '../conversations/Thread.js';
import { useLayout } from '../page/useLayout.js';
import { dockAt, pageLook, Shown, type Display } from './display.js';
import { sample } from './sample.js';

// Message display's preview: the Communications page as `display` has it look, drawn by the
// page's own parts with a made-up morning (`sample`), on a screen `width` by `height` (a phone's
// or a desktop's, for the page's layouts by width), Jamal's profile open or not, beside the
// conversation where the page would have it. A picture: nothing in it can be used.

const nothing = () => undefined;

export function Preview({
  display,
  phone,
  profile,
  width,
  height,
  timeZone,
}: {
  display: Display;
  phone: boolean;
  /** Whether Jamal's profile is open. */
  profile: boolean;
  width: number;
  height: number;
  timeZone: string;
}) {
  const now = useMemo(() => Date.now(), []);
  const {
    inbox,
    open,
    messages,
    profile: known,
    workspace,
    limits,
  } = useMemo(() => sample(now), [now]);
  const title = useId();
  const page = useRef<HTMLDivElement>(null);
  const docked =
    useLayout(page, dockAt(display)) === 'docked' && display.profile_opens === 'beside';
  const about = [roleText(open.with), workspace.name].join(' · ');
  return (
    <div
      className={phone ? 'display-screen phone' : 'display-screen'}
      style={{ width, height }}
      inert
    >
      {/* On a phone, the conversation open, which takes the page. */}
      <div
        className="communications reading"
        {...pageLook(display)}
        data-docked={docked || undefined}
      >
        <Shown value={display}>
          <div ref={page} className="communications-page">
            <div className="chat-frame">
              <ListPane
                inbox={inbox}
                routes={{ groups: [], read: { pending: false, error: '' }, choose: nothing }}
                sections={['slack']}
                section="slack"
                chooseSection={nothing}
                working={inbox.workspaces}
                choose={nothing}
                current={open}
                readOlder={nothing}
                reach={() => Promise.resolve()}
                show={nothing}
                open={open.id}
                typing={[]}
                timeZone={timeZone}
                now={now}
                select={nothing}
                openGroup={nothing}
                done={nothing}
                doneGroup={nothing}
              />
              <section
                className={profile ? 'pane-thread profiled' : 'pane-thread'}
                aria-label={open.with.name}
              >
                <PaneHead
                  face={<PartyFace conversation={open} size="head" />}
                  name={open.with.name}
                  about={about}
                  waits={open.state === 'waiting'}
                  waitingSince={open.waitingSince}
                  now={now}
                  open={profile}
                  opens={{}}
                  back={nothing}
                  done={{ busy: false, run: nothing }}
                />
                {profile && (
                  <aside className="panel profile" aria-labelledby={title}>
                    <ProfileShown
                      title={title}
                      person={open.with}
                      profile={known}
                      error=""
                      workspace={workspace}
                      timeZone={timeZone}
                    />
                  </aside>
                )}
                <Thread
                  conversation={open}
                  messages={messages}
                  more={false}
                  readOlder={nothing}
                  writing={[]}
                  timeZone={timeZone}
                  now={now}
                  profile={{}}
                  openGroup={nothing}
                />
                <Composer
                  placeholder={`Message ${open.with.name.split(' ')[0]}`}
                  writable
                  limits={limits}
                  attachments={slackAttachments}
                  send={() => Promise.resolve()}
                />
              </section>
            </div>
          </div>
        </Shown>
      </div>
    </div>
  );
}
