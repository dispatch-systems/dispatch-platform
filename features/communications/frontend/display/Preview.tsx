import { useId, useMemo } from 'react';
import { Composer } from '../page/Composer.js';
import { ConversationHead } from '../page/ConversationHead.js';
import { ListPane } from '../page/ListPane.js';
import { roleText } from '../page/parts.js';
import { ProfileShown } from '../page/Profile.js';
import { Thread } from '../page/Thread.js';
import { pageLook, Shown, type Display } from './display.js';
import { sample } from './sample.js';

// Message display's preview: the Communications page as `display` has it look, drawn by the
// page's own parts with a made-up morning (`sample`), on a screen `width` by `height` (a phone's
// or a desktop's, for the page's layouts by width), Jamal's profile open or not. A picture:
// nothing in it can be used.

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
  const { inbox, open, messages, profile: known, workspace } = useMemo(() => sample(now), [now]);
  const title = useId();
  const about = [roleText(open.with), workspace.name].join(' · ');
  return (
    <div
      className={phone ? 'display-screen phone' : 'display-screen'}
      style={{ width, height }}
      inert
    >
      {/* On a phone, the conversation open, which takes the page. */}
      <div className="communications reading" {...pageLook(display)}>
        <Shown value={display}>
          <div className="communications-page">
            <div className="chat-frame">
              <ListPane
                inbox={inbox}
                working={inbox.workspaces}
                choose={nothing}
                current={open}
                readOlder={nothing}
                pick={() => Promise.resolve()}
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
                <ConversationHead
                  conversation={open}
                  about={about}
                  now={now}
                  open={profile}
                  opens={{}}
                  back={nothing}
                  done={{ busy: false, run: nothing }}
                />
                {profile && (
                  <aside className="profile" aria-labelledby={title}>
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
