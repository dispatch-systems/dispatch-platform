import { useState } from 'react';
import { SlidersHorizontal } from 'lucide-react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import { useFillsWindow } from '../../../../core/shell/frontend/runtime/fills.js';
import { usePushKey } from '../../api/client.js';
import type { MessageDisplay } from '../../api/index.js';
import { DisplayPreview } from './DisplayPreview.js';
import { useDisplayChoices } from './useDisplayChoices.js';
import { Choice, Segments, Sketch, Slider, Swatches, Switch, Tiles } from './controls.js';
import {
  kept,
  lookOf,
  looks,
  lookSizes,
  onScreen,
  screenDefaults,
  thisScreen,
  type Display,
  type Look,
  type Screen,
} from './display.js';
import { allowed as notificationsAllowed, canNotify } from '../alerts/notices.js';
import { canPush } from '../alerts/push.js';
import '../communications.css';
import './display.css';

// Settings' Message display: each of the member's choices of how Communications looks, a
// desktop's or a phone's own (the screen in use to start), beside a preview of the page with them,
// which changes as they choose. A phone has the choices that apply on one. What they choose is
// shown at once and saved a moment later, their own; Reset takes the screen back to its defaults.
// Then how a new message alerts them, on any screen.
// The preview shows the screen chosen at its actual size, or a desktop's whole page, as big as
// it is in this window, made to fit, as it starts (`frame`).

// The choices scroll beside the preview while both fit in the window (display.css).
const fillsWindow = '(width > 1240px) and (height >= 600px)';

export function DisplaySettings({ view, refresh }: { view: DspView; refresh: () => void }) {
  useFillsWindow(fillsWindow);
  const choices = useDisplayChoices(refresh);
  const [screen, setScreen] = useState<Screen>(thisScreen);
  const shown = onScreen(choices.values, screen);
  const change = (changes: Partial<Display>) => choices.change(kept(changes, screen));
  const phone = screen === 'phone';
  // Notifications are offered where this browser shows them from the page, or from pushes the
  // server sends, and on only where it allows them, which turning them on asks.
  const { data: push } = usePushKey();
  const offered = canNotify() || (canPush() && Boolean(push?.key));
  const [allowed, setAllowed] = useState(notificationsAllowed);
  const notifications = async (on: boolean) => {
    if (on) {
      const answer = notificationsAllowed() ? 'granted' : await Notification.requestPermission();
      setAllowed(answer === 'granted');
      if (answer !== 'granted') return;
    }
    choices.change({ notifications: on });
  };
  const look = lookOf(shown);
  const bubbles = shown.style === 'bubbles';
  const sections = [
    ['look', 'Look'],
    ['text', 'Text'],
    ['messages', 'Messages'],
    ['colors', 'Colors'],
    ['list', 'List'],
    ['profile', 'Profile'],
    ['writing', 'Writing'],
    ['alerts', 'New messages'],
  ] as const;
  const at = (id: string) => `message-display-${id}`;
  return (
    <div className="message-display">
      <div className="display-choices">
        <div className="display-screens">
          <Choice
            label="Screen"
            value={screen}
            options={[
              ['desktop', 'Desktop'],
              ['phone', 'Phone'],
            ]}
            change={setScreen}
          />
        </div>
        <nav className="display-jump" aria-label="Message display">
          {sections.map(([id, label]) => (
            <button
              key={id}
              onClick={() => document.getElementById(at(id))?.scrollIntoView({ block: 'start' })}
            >
              {label}
            </button>
          ))}
        </nav>

        <div className="display-sections">
          <section className="settings-section" id={at('look')} aria-label="Look">
            <div className="display-section-head">
              <h2>Look</h2>
              <button onClick={() => change(screenDefaults(screen))}>Reset</button>
            </div>
            <div className="display-looks" role="radiogroup" aria-label="Look">
              {(Object.keys(looks) as Look[]).map((each) => (
                <label key={each} className="display-look">
                  <input
                    type="radio"
                    name="message-display-look"
                    checked={look === each}
                    onChange={() => change(lookSizes(each))}
                  />
                  <b style={{ fontSize: lookSizes(each).text_size + 4 }}>Aa</b>
                  <span>{looks[each].label}</span>
                </label>
              ))}
              {!look && (
                <label className="display-look">
                  <input type="radio" name="message-display-look" checked readOnly />
                  <b>
                    <SlidersHorizontal size={20} aria-hidden="true" />
                  </b>
                  <span>Custom</span>
                </label>
              )}
            </div>
          </section>

          <section className="settings-section" id={at('text')} aria-label="Text">
            <h2>Text</h2>
            <div className="settings-fields">
              <Slider
                label="Text size"
                value={shown.text_size}
                min={12}
                max={20}
                unit="px"
                ends={[
                  'A',
                  <span key="large" className="display-large-a">
                    A
                  </span>,
                ]}
                change={(text_size) => change({ text_size })}
              />
              <Segments
                label="Line spacing"
                value={shown.line_spacing}
                options={[
                  ['tight', 'Tight'],
                  ['normal', 'Normal'],
                  ['relaxed', 'Relaxed'],
                ]}
                change={(line_spacing) => change({ line_spacing })}
              />
              <Segments
                label="Spacing"
                value={shown.spacing}
                options={[
                  ['compact', 'Compact'],
                  ['comfortable', 'Comfortable'],
                  ['spacious', 'Spacious'],
                ]}
                change={(spacing) => change({ spacing })}
              />
            </div>
          </section>

          <section className="settings-section" id={at('messages')} aria-label="Messages">
            <h2>Messages</h2>
            <div className="settings-fields">
              {!phone && (
                <Tiles
                  label="Layout"
                  value={shown.layout}
                  options={[
                    ['full', 'Full width', <Sketch key="full" kind="full" />],
                    ['centred', 'Centred', <Sketch key="centred" kind="centred" />],
                  ]}
                  change={(layout) => change({ layout })}
                />
              )}
              {!phone && shown.layout === 'centred' && (
                <Slider
                  label="Column width"
                  value={shown.column_width}
                  min={600}
                  max={1200}
                  step={50}
                  unit="px"
                  change={(column_width) => change({ column_width })}
                />
              )}
              <Tiles
                label="Style"
                value={shown.style}
                options={[
                  ['bubbles', 'Bubbles', <Sketch key="bubbles" kind="bubbles" />],
                  ['plain', 'Plain', <Sketch key="plain" kind="plain" />],
                ]}
                change={(style) => change({ style })}
              />
              {bubbles && (
                <>
                  <Segments
                    label="Your team"
                    value={shown.team_side}
                    options={[
                      ['right', 'Right'],
                      ['left', 'Left'],
                    ]}
                    change={(team_side) => change({ team_side })}
                  />
                  <Segments
                    label="Bubble shape"
                    value={shown.bubble_shape}
                    options={[
                      ['round', 'Round'],
                      ['soft', 'Soft'],
                      ['square', 'Square'],
                    ]}
                    change={(bubble_shape) => change({ bubble_shape })}
                  />
                  <Slider
                    label="Bubble width"
                    value={shown.bubble_width}
                    min={phone ? 60 : 50}
                    max={phone ? 100 : 90}
                    step={5}
                    unit="%"
                    change={(bubble_width) => change({ bubble_width })}
                  />
                </>
              )}
              <Segments
                label="Times"
                wide
                value={shown.times}
                options={[
                  ['each_message', 'Each message'],
                  ['each_group', 'Each group'],
                  // A phone has no hover.
                  ...(phone ? [] : [['on_hover', 'On hover'] as const]),
                  ['after_pause', 'After a pause'],
                ]}
                change={(times) => change({ times })}
              />
              <Switch
                label="Photos"
                on={shown.message_photos}
                change={(message_photos) => change({ message_photos })}
              />
              {bubbles && (
                <Switch
                  label="Team names"
                  on={shown.team_names}
                  change={(team_names) => change({ team_names })}
                />
              )}
            </div>
          </section>

          <section className="settings-section" id={at('colors')} aria-label="Colors">
            <h2>Colors</h2>
            <div className="settings-fields">
              <Swatches
                label="Your team"
                of="team"
                value={shown.team_color}
                options={[
                  ['blue', 'Blue'],
                  ['indigo', 'Indigo'],
                  ['teal', 'Teal'],
                  ['green', 'Green'],
                  ['purple', 'Purple'],
                  ['slate', 'Slate'],
                ]}
                change={(team_color) => change({ team_color })}
              />
              <Swatches
                label="Employees"
                of="employee"
                value={shown.employee_color}
                options={[
                  ['white', 'White'],
                  ['gray', 'Gray'],
                  ['blue', 'Blue'],
                  ['warm', 'Warm'],
                  ['green', 'Green'],
                ]}
                change={(employee_color) => change({ employee_color })}
              />
            </div>
          </section>

          <section className="settings-section" id={at('list')} aria-label="Conversation list">
            <h2>Conversation list</h2>
            <div className="settings-fields">
              {!phone && (
                <Segments
                  label="Width"
                  value={shown.list_width}
                  options={[
                    ['narrow', 'Narrow'],
                    ['medium', 'Medium'],
                    ['wide', 'Wide'],
                  ]}
                  change={(list_width) => change({ list_width })}
                />
              )}
              <Segments
                label="Rows"
                value={shown.list_rows}
                options={[
                  ['compact', 'Compact'],
                  ['comfortable', 'Comfortable'],
                ]}
                change={(list_rows) => change({ list_rows })}
              />
              <Segments
                label="Preview"
                value={shown.list_preview}
                options={[
                  ['none', 'None'],
                  ['one', '1 line'],
                  ['two', '2 lines'],
                ]}
                change={(list_preview) => change({ list_preview })}
              />
              <Switch
                label="Photos"
                on={shown.list_photos}
                change={(list_photos) => change({ list_photos })}
              />
              <Switch
                label="Waiting time"
                on={shown.waiting_time}
                change={(waiting_time) => change({ waiting_time })}
              />
            </div>
          </section>

          <section className="settings-section" id={at('profile')} aria-label="Profile">
            <h2>Profile</h2>
            <div className="settings-fields">
              {!phone && (
                <>
                  <Tiles
                    label="Opens"
                    value={shown.profile_opens}
                    options={[
                      ['beside', 'Beside', <Sketch key="beside" kind="beside" />],
                      ['over', 'Over', <Sketch key="over" kind="over" />],
                    ]}
                    change={(profile_opens) => change({ profile_opens })}
                  />
                  <Slider
                    label="Width"
                    value={shown.profile_width}
                    min={280}
                    max={420}
                    step={10}
                    unit="px"
                    scale={shown.text_size / 14}
                    change={(profile_width) => change({ profile_width })}
                  />
                </>
              )}
              <Switch
                label="Slide in"
                on={shown.profile_slide}
                change={(profile_slide) => change({ profile_slide })}
              />
              {!phone && (
                <Switch
                  label="Keep profile open"
                  on={shown.keep_profile_open}
                  change={(keep_profile_open) => change({ keep_profile_open })}
                />
              )}
            </div>
          </section>

          <section className="settings-section" id={at('writing')} aria-label="Writing">
            <h2>Writing</h2>
            <div className="settings-fields">
              <Switch
                label="Enter sends"
                on={shown.enter_sends}
                change={(enter_sends) => change({ enter_sends })}
              />
            </div>
          </section>

          <section className="settings-section" id={at('alerts')} aria-label="New messages">
            <h2>New messages</h2>
            <div className="settings-fields">
              <Switch
                label="Sound"
                on={choices.values.sound}
                change={(sound) => choices.change({ sound })}
              />
              {offered && (
                <Switch
                  label="Notifications"
                  on={choices.values.notifications && allowed}
                  change={(on) => void notifications(on)}
                />
              )}
            </div>
          </section>
        </div>
      </div>

      <DisplayPreview display={shown} phone={phone} timeZone={view.dsp.timezone} />
    </div>
  );
}
