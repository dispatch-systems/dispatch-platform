import { useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { Choice } from './controls.js';
import { widths, type Display } from './display.js';
import { ACTUAL_LEAST, frame } from './frame.js';
import { Preview } from './Preview.js';

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

/** Below the Communications page's heading, to its list and conversation (communications.css). */
const UNDER_HEADING = 22;

/**
 * The size of the Communications page's list and conversation in this window, as it changes: the
 * room beside the app's sidebar, under its bars and a page's heading, as `box`'s page, Settings,
 * has them too.
 */
function usePage(box: RefObject<HTMLElement | null>) {
  const [page, setPage] = useState({ width: 0, height: 0 });
  useLayoutEffect(() => {
    const main = box.current?.closest('main');
    const area = main?.parentElement;
    const heading = main?.querySelector('.page-heading');
    if (!area || !heading) return;
    const measure = () =>
      setPage({
        width: window.innerWidth - area.getBoundingClientRect().left,
        height:
          window.innerHeight -
          (heading.getBoundingClientRect().bottom + window.scrollY + UNDER_HEADING),
      });
    // The window resized, or the app's bars or the page's heading changed size.
    const observer = new ResizeObserver(measure);
    observer.observe(area);
    window.addEventListener('resize', measure);
    return () => {
      observer.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, [box]);
  return page;
}

/** The preview's frame and measurements, independent of preference editing. */
export function DisplayPreview({
  display: shown,
  phone,
  timeZone,
}: {
  display: Display;
  phone: boolean;
  timeZone: string;
}) {
  const [size, setSize] = useState<'actual' | 'fit'>('fit');
  const [profile, setProfile] = useState(false);
  const stage = useRef<HTMLDivElement>(null);
  const room = useRoom(stage);
  const page = usePage(stage);
  const { width, height, scale, aside } = frame({
    room,
    phone,
    fit: size === 'fit',
    list: widths(shown).list,
    page,
  });
  return (
    <div className="display-preview">
      <div className="display-bar">
        <label className="settings-toggle display-bar-switch">
          <span>Profile</span>
          <input
            type="checkbox"
            role="switch"
            checked={profile}
            onChange={(event) => setProfile(event.target.checked)}
          />
        </label>
        {!phone && room.width >= ACTUAL_LEAST && (
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
        {room.width > 0 && page.width > 0 && (
          <div
            className={phone ? 'display-scaled phone' : 'display-scaled'}
            style={{
              width: Math.min(width * scale, room.width),
              height: Math.min(height * scale, room.height),
            }}
          >
            <div
              className="display-scaler"
              style={{ transform: `translateX(${-aside}px) scale(${scale})` }}
            >
              <Preview
                display={shown}
                phone={phone}
                profile={profile}
                width={width}
                height={height}
                timeZone={timeZone}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
