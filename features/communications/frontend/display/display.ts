import { createContext, useSyncExternalStore, type CSSProperties } from 'react';
import type { MessageDisplay } from '../../api/index.js';
import type { SettingsAnswer } from '../../../../core/settings/api/index.js';
import { useSettings } from '../../../../core/settings/api/client.js';

// How the page looks to the member: what they chose in Settings' Message display, their own,
// which the page and its preview there draw by. A desktop and a phone each have their own: on a
// phone, the choices that apply on one, kept as `phone_<choice>`. Sizes go by the text size; the
// rest are classes of the page's own (`data-*`), which each part's stylesheet draws.

/** Its section of Settings, as Communications' backend declares it. */
export const section = 'communications.display';

/** What a member has before choosing anything, as the backend declares it. */
export const defaults: MessageDisplay = {
  text_size: 14,
  line_spacing: 'normal',
  spacing: 'comfortable',
  layout: 'full',
  column_width: 800,
  style: 'bubbles',
  team_side: 'right',
  bubble_shape: 'round',
  bubble_width: 70,
  times: 'each_group',
  message_photos: true,
  team_names: true,
  team_color: 'blue',
  employee_color: 'gray',
  list_width: 'medium',
  list_rows: 'comfortable',
  list_preview: 'one',
  list_photos: true,
  waiting_time: true,
  profile_opens: 'beside',
  profile_width: 320,
  profile_slide: true,
  keep_profile_open: false,
  enter_sends: true,
  sound: true,
  notifications: false,
  phone_text_size: 14,
  phone_line_spacing: 'normal',
  phone_spacing: 'comfortable',
  phone_style: 'bubbles',
  phone_team_side: 'right',
  phone_bubble_shape: 'round',
  phone_bubble_width: 80,
  phone_times: 'each_group',
  phone_message_photos: true,
  phone_team_names: true,
  phone_team_color: 'blue',
  phone_employee_color: 'gray',
  phone_list_rows: 'comfortable',
  phone_list_preview: 'one',
  phone_list_photos: true,
  phone_waiting_time: true,
  phone_profile_slide: true,
  phone_enter_sends: false,
};

/** Which screen's choices: a desktop's (or laptop's or tablet's) or a phone's. */
export type Screen = 'desktop' | 'phone';
/** How a new message alerts the member, on any screen: not how the page looks. */
type Alerting = 'sound' | 'notifications';
const alerting = new Set<string>(['sound', 'notifications'] satisfies Alerting[]);
/** What the page draws by on one screen. */
export type Display = Omit<MessageDisplay, `phone_${string}` | Alerting>;
/** A choice a phone has of its own, kept as `phone_<choice>`. */
type PhoneChoice = keyof MessageDisplay extends infer Key
  ? Key extends `phone_${infer Choice extends keyof Display}`
    ? Choice
    : never
  : never;
const phoneChoices = Object.keys(defaults).flatMap((key) =>
  key.startsWith('phone_') ? [key.slice('phone_'.length) as PhoneChoice] : [],
);
/**
 * What a phone draws by of what it has no choice of: one pane at a time, which these don't
 * change.
 */
const onAPhone = {
  layout: 'full',
  column_width: 800,
  list_width: 'medium',
  profile_opens: 'over',
  profile_width: 320,
  keep_profile_open: false,
} satisfies Omit<Display, PhoneChoice>;

/** What the page draws by on `screen`, of what Settings keeps: that screen's alone. */
export function onScreen(values: MessageDisplay, screen: Screen): Display {
  if (screen === 'desktop')
    return Object.fromEntries(
      Object.entries(values).filter(([key]) => !key.startsWith('phone_') && !alerting.has(key)),
    ) as Display;
  const own = Object.fromEntries(phoneChoices.map((choice) => [choice, values[`phone_${choice}`]]));
  return { ...onAPhone, ...own } as Display;
}
/** `changes` made to `screen`'s choices, as Settings keeps them. */
export function kept(changes: Partial<Display>, screen: Screen): Partial<MessageDisplay> {
  if (screen === 'desktop') return changes;
  return Object.fromEntries(Object.entries(changes).map(([key, value]) => [`phone_${key}`, value]));
}
/** What `screen` has before choosing anything: each of its choices. */
export function screenDefaults(screen: Screen): Partial<Display> {
  const own = onScreen(defaults, screen);
  if (screen === 'desktop') return own;
  return Object.fromEntries(phoneChoices.map((choice) => [choice, own[choice]]));
}

/** The member's own, as Settings read them; the defaults until it has. */
export const ownDisplay = (settings: SettingsAnswer | undefined): MessageDisplay => {
  const kept = settings?.sections.find((each) => each.id === section)?.values;
  return { ...defaults, ...(kept as Partial<MessageDisplay> | undefined) };
};

/** A phone: a touch screen whose shorter side is under 500px, either way up. */
const phoneQuery =
  '(pointer: coarse) and (max-width: 499px), (pointer: coarse) and (max-height: 499px)';
const watchPhone = (changed: () => void) => {
  const query = matchMedia(phoneQuery);
  query.addEventListener('change', changed);
  return () => query.removeEventListener('change', changed);
};
/** Whether this is a phone, as it changes. */
export const usePhone = () =>
  useSyncExternalStore(watchPhone, () => matchMedia(phoneQuery).matches);
/** The screen this is. */
export const thisScreen = (): Screen => (matchMedia(phoneQuery).matches ? 'phone' : 'desktop');

/** The member's own on this screen, read with Settings, which a save there updates at once. */
export function useDisplay() {
  const { data } = useSettings();
  return onScreen(ownDisplay(data), usePhone() ? 'phone' : 'desktop');
}

/** What the page draws by, for the parts that read it as they draw: the thread, the box. */
export const Shown = createContext<Display>(onScreen(defaults, 'desktop'));

/** The sizes a look sets, leaving the member's other choices as they are. */
const sized = ['text_size', 'spacing', 'list_rows'] as const;
type Sized = Pick<Display, (typeof sized)[number]>;
/** Whole looks to start from: the sizes of each, the defaults' others. */
export const looks = {
  default: { label: 'Default', sizes: {} },
  compact: { label: 'Compact', sizes: { text_size: 13, spacing: 'compact', list_rows: 'compact' } },
  comfortable: { label: 'Comfortable', sizes: { text_size: 15, spacing: 'spacious' } },
  large: { label: 'Large', sizes: { text_size: 17 } },
} satisfies Record<string, { label: string; sizes: Partial<Sized> }>;
export type Look = keyof typeof looks;
/** The sizes `look` sets, every one of them. */
export const lookSizes = (look: Look): Sized => {
  const sizes: Partial<Sized> = looks[look].sizes;
  return Object.fromEntries(sized.map((key) => [key, sizes[key] ?? defaults[key]])) as Sized;
};
/** The look whose sizes `display` has; none once they're the member's own. */
export const lookOf = (display: Display) =>
  (Object.keys(looks) as Look[]).find((look) => {
    const sizes = lookSizes(look);
    return sized.every((key) => display[key] === sizes[key]);
  });

const spacing = { compact: 0.67, comfortable: 1, spacious: 1.33 };
const line = { tight: 1.3, normal: 1.42, relaxed: 1.62 };
/** The list's width, by its choice. */
export const listWidths = { narrow: 300, medium: 340, wide: 400 };
const radius = { round: '1.3em', soft: '0.75em', square: '0.3em' };

/** The list's and the profile's widths as drawn: as chosen at 14px, growing with the text. */
export const widths = (display: Display) => {
  const scale = display.text_size / 14;
  return { list: listWidths[display.list_width] * scale, profile: display.profile_width * scale };
};
/**
 * How wide the page is from which an open profile, beside the conversation, is a column of its
 * own: room for the list, the profile, and the conversation between them, each growing with the
 * text. At 14px, 1150px.
 */
export function dockAt(display: Display) {
  const { list, profile } = widths(display);
  return list + profile + (490 * display.text_size) / 14;
}

/**
 * What the page sets on itself to look as `display` has it: its sizes, and the classes its parts'
 * stylesheets draw the rest by.
 */
export function pageLook(display: Display) {
  const { list, profile } = widths(display);
  const style = {
    '--text-size': `${display.text_size}px`,
    '--scale': display.text_size / 14,
    '--spacing': spacing[display.spacing],
    '--line': line[display.line_spacing],
    '--list-width': `${list}px`,
    '--column-width': display.layout === 'centred' ? `${display.column_width}px` : '100%',
    '--bubble-width': `${display.bubble_width}%`,
    // A bubble's line, as readable at any width, grows and shrinks with the width chosen.
    '--bubble-share': display.bubble_width / 70,
    '--bubble-radius': radius[display.bubble_shape],
    '--profile-width': `${profile}px`,
    ...(display.profile_slide ? {} : { '--profile-slide': '0s' }),
  } as CSSProperties;
  const hidden = [
    !display.message_photos && 'message-photos',
    !display.team_names && 'team-names',
    !display.list_photos && 'list-photos',
    !display.waiting_time && 'waiting-time',
  ].filter(Boolean);
  return {
    style,
    'data-style': display.style,
    'data-team-side': display.team_side,
    'data-times': display.times,
    'data-team-color': display.team_color,
    'data-employee-color': display.employee_color,
    'data-list-rows': display.list_rows,
    'data-list-preview': display.list_preview,
    'data-profile': display.profile_opens,
    'data-hide': hidden.join(' ') || undefined,
  };
}
