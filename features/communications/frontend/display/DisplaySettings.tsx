import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { SlidersHorizontal } from 'lucide-react';
import type { DspView } from '../../../../core/accounts/api/index.js';
import { saveSettings, useSettings } from '../../../../core/settings/api/client.js';
import { ApiError } from '../../../../core/shell/frontend/runtime/api.js';
import { dataCache } from '../../../../core/shell/frontend/runtime/data-cache.js';
import { useAction } from '../../../../core/shell/frontend/runtime/useAction.js';
import type { MessageDisplay } from '../../api/index.js';
import { Choice, Segments, Sketch, Slider, Swatches, Switch, Tiles } from './controls.js';
import { defaults, lookOf, looks, lookSizes, ownDisplay, section, type Look } from './display.js';
import { Preview } from './Preview.js';
import { autosave } from './saving.js';
import '../communications.css';
import './display.css';

// Settings' Message display: each of the member's choices of how Communications looks, beside a
// preview of the page with them, which changes as they choose. What they choose is shown at once
// and saved a moment later, their own; Reset goes back to the defaults. The preview shows a
// desktop as wide as the room beside the choices, at its actual size, while that's room for the
// list beside the conversation; or as wide as the member's window, made to fit; or a phone, at its
// actual size, whose text size is its own.

/** How long after a choice it's saved, so a slider dragged saves once. */
const SAVE_MS = 400;
/** The app's sidebar beside the page, and its heading above it, on a desktop. */
const SIDEBAR = 240;
const HEADING = 150;
const PHONE = { width: 390, height: 760 };
/** A phone's edge, round its screen. */
const PHONE_EDGE = 20;
/** The least room a desktop is shown in at its actual size: the list beside the conversation. */
const ACTUAL_LEAST = 900;

/**
 * The member's choices as they make them, saved by `autosave` through the page's action, which says
 * why a save failed at the top of the page until one succeeds; one refused because another session
 * saved first reads Settings again, and what it holds replaces the choices made.
 */
function useChoices(refresh: () => void) {
  const { data: settings } = useSettings();
  const saved = ownDisplay(settings);
  const kept = settings?.sections.find((each) => each.id === section);
  const [draft, setDraft] = useState<MessageDisplay>();
  const action = useAction((work: () => Promise<void>) => work());
  // The newest of each, for saves that run after the page draws again.
  const run = useRef(action.run);
  run.current = action.run;
  const rereading = useRef(refresh);
  rereading.current = refresh;
  const [saver] = useState(() =>
    autosave({
      save: async (values, revision) => {
        const answer = await saveSettings(section, { values, revision });
        return 'section' in answer ? answer.section.revision : revision;
      },
      view: () => dataCache.session,
      revision: kept?.revision ?? 0,
      perform: (work) => run.current(work),
      later: (then) => {
        const timer = window.setTimeout(then, SAVE_MS);
        return () => window.clearTimeout(timer);
      },
      shown: setDraft,
      clash: (error) =>
        error instanceof ApiError && error.code === 'settings_changed_reload_before_saving',
      reread: () => rereading.current(),
    }),
  );
  useEffect(() => {
    if (kept) saver.reread(kept.revision);
  }, [kept, saver]);
  // Leaving the page saves what waits.
  useEffect(() => () => void saver.flush(), [saver]);
  const change = (changes: Partial<MessageDisplay>) =>
    saver.change({ ...(saver.draft ?? saved), ...changes });
  return { shown: draft ?? saved, change };
}

/** The size of what `box` holds, as it changes. */
function useRoom(box: RefObject<HTMLElement | null>) {
  const [room, setRoom] = useState({ width: 0, height: 0 });
  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) =>
      setRoom({ width: entry!.contentRect.width, height: entry!.contentRect.height }),
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [box]);
  return room;
}

export function DisplaySettings({ view, refresh }: { view: DspView; refresh: () => void }) {
  const { shown, change } = useChoices(refresh);
  const [screen, setScreen] = useState<'desktop' | 'phone'>(() =>
    matchMedia('(width <= 760px)').matches ? 'phone' : 'desktop',
  );
  const [size, setSize] = useState<'actual' | 'fit'>('actual');
  const [profile, setProfile] = useState(false);
  const stage = useRef<HTMLDivElement>(null);
  const room = useRoom(stage);
  const phone = screen === 'phone';
  // The screen previewed, and how much it's made smaller to fit the room it has.
  const roomy = room.width >= ACTUAL_LEAST;
  const actual = !phone && size === 'actual' && roomy;
  // A phone, always at its actual size, in as much of one as there's room for (inside its edge).
  const whole = phone
    ? {
        width: Math.min(PHONE.width, room.width - PHONE_EDGE),
        height: Math.min(PHONE.height, room.height - PHONE_EDGE),
      }
    : actual
      ? room
      : { width: window.innerWidth - SIDEBAR, height: window.innerHeight - HEADING };
  const scale =
    phone || actual ? 1 : Math.min(1, room.width / whole.width, room.height / whole.height);
  const look = lookOf(shown);
  const textSize = phone ? 'phone_text_size' : 'text_size';
  const bubbles = shown.style === 'bubbles';
  const sections = [
    ['look', 'Look'],
    ['text', 'Text'],
    ['messages', 'Messages'],
    ['colors', 'Colors'],
    ['list', 'List'],
    ['profile', 'Profile'],
    ['writing', 'Writing'],
  ] as const;
  const at = (id: string) => `message-display-${id}`;
  return (
    <div className="message-display">
      <div className="display-choices">
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

        <section className="settings-section" id={at('look')} aria-label="Look">
          <div className="display-section-head">
            <h2>Look</h2>
            <button onClick={() => change(defaults)}>Reset</button>
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
              value={shown[textSize]}
              min={12}
              max={20}
              unit="px"
              ends={[
                'A',
                <span key="large" className="display-large-a">
                  A
                </span>,
              ]}
              change={(value) => change({ [textSize]: value })}
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
            <Tiles
              label="Layout"
              value={shown.layout}
              options={[
                ['full', 'Full width', <Sketch key="full" kind="full" />],
                ['centred', 'Centred', <Sketch key="centred" kind="centred" />],
              ]}
              change={(layout) => change({ layout })}
            />
            {shown.layout === 'centred' && (
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
                  min={50}
                  max={90}
                  step={5}
                  unit="%"
                  change={(bubble_width) => change({ bubble_width })}
                />
              </>
            )}
            <Segments
              label="Times"
              value={shown.times}
              options={[
                ['each_message', 'Each message'],
                ['each_group', 'Each group'],
                ['on_hover', 'On hover'],
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
              change={(profile_width) => change({ profile_width })}
            />
            <Switch
              label="Slide in"
              on={shown.profile_slide}
              change={(profile_slide) => change({ profile_slide })}
            />
            <Switch
              label="Keep profile open"
              on={shown.keep_profile_open}
              change={(keep_profile_open) => change({ keep_profile_open })}
            />
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
      </div>

      <div className="display-preview">
        <div className="display-bar">
          <Choice
            label="Screen"
            value={screen}
            options={[
              ['desktop', 'Desktop'],
              ['phone', 'Phone'],
            ]}
            change={setScreen}
          />
          <label className="settings-toggle display-bar-switch">
            <span>Profile</span>
            <input
              type="checkbox"
              role="switch"
              checked={profile}
              onChange={(event) => setProfile(event.target.checked)}
            />
          </label>
          {!phone && roomy && (
            <Choice
              label="Size"
              value={size}
              options={[
                ['actual', 'Actual size'],
                ['fit', 'Fit'],
              ]}
              change={setSize}
            />
          )}
        </div>
        <div className="display-stage" ref={stage}>
          {room.width > 0 && (
            <div
              className={phone ? 'display-scaled phone' : 'display-scaled'}
              style={{
                width: Math.min(whole.width * scale, room.width),
                height: Math.min(whole.height * scale, room.height),
              }}
            >
              <div className="display-scaler" style={{ transform: `scale(${scale})` }}>
                <Preview
                  display={shown}
                  phone={phone}
                  profile={profile}
                  width={whole.width}
                  height={whole.height}
                  timeZone={view.dsp.timezone}
                />
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
