import { createContext, type CSSProperties } from 'react';
import type { MessageDisplay } from '../../api/index.js';
import type { SettingsAnswer } from '../../../../core/settings/api/index.js';
import { useSettings } from '../../../../core/settings/api/client.js';

// How the page looks to the member: what they chose in Settings' Message display, their own,
// which the page and its preview there draw by. Sizes go by the text size; the rest are classes
// of the page's own (`data-*`), which communications.css draws.

/** Its section of Settings, as Communications' backend declares it. */
export const section = 'communications.display';

/** What a member has before choosing anything, as the backend declares it. */
export const defaults: MessageDisplay = {
  text_size: 14,
  phone_text_size: 14,
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
};

/** The member's own, as Settings read them; the defaults until it has. */
export const ownDisplay = (settings: SettingsAnswer | undefined): MessageDisplay => {
  const kept = settings?.sections.find((each) => each.id === section)?.values;
  return { ...defaults, ...(kept as Partial<MessageDisplay> | undefined) };
};
/** The member's own, read with Settings, which a save there updates at once. */
export function useDisplay() {
  const { data } = useSettings();
  return ownDisplay(data);
}

/** What the page draws by, for the parts that read it as they draw: the thread, the box. */
export const Shown = createContext<MessageDisplay>(defaults);

/** The sizes a look sets, leaving the member's other choices as they are. */
const sized = ['text_size', 'phone_text_size', 'spacing', 'list_rows'] as const;
type Sized = Pick<MessageDisplay, (typeof sized)[number]>;
/** Whole looks to start from: the sizes of each, the defaults' others. */
export const looks = {
  default: { label: 'Default', sizes: {} },
  compact: {
    label: 'Compact',
    sizes: { text_size: 13, phone_text_size: 13, spacing: 'compact', list_rows: 'compact' },
  },
  comfortable: {
    label: 'Comfortable',
    sizes: { text_size: 15, phone_text_size: 15, spacing: 'spacious' },
  },
  large: { label: 'Large', sizes: { text_size: 17, phone_text_size: 17 } },
} satisfies Record<string, { label: string; sizes: Partial<Sized> }>;
export type Look = keyof typeof looks;
/** The sizes `look` sets, every one of them. */
export const lookSizes = (look: Look): Sized => {
  const sizes: Partial<Sized> = looks[look].sizes;
  return Object.fromEntries(sized.map((key) => [key, sizes[key] ?? defaults[key]])) as Sized;
};
/** The look whose sizes `display` has; none once they're the member's own. */
export const lookOf = (display: MessageDisplay) =>
  (Object.keys(looks) as Look[]).find((look) => {
    const sizes = lookSizes(look);
    return sized.every((key) => display[key] === sizes[key]);
  });

const spacing = { compact: 0.67, comfortable: 1, spacious: 1.33 };
const line = { tight: 1.3, normal: 1.42, relaxed: 1.62 };
const listWidth = { narrow: 300, medium: 340, wide: 400 };
const radius = { round: '1.3em', soft: '0.75em', square: '0.3em' };

/**
 * What the page sets on itself to look as `display` has it, on a phone or not: its sizes, and
 * the classes communications.css draws the rest by.
 */
export function pageLook(display: MessageDisplay, phone: boolean) {
  const text = phone ? display.phone_text_size : display.text_size;
  const style = {
    '--text-size': `${text}px`,
    '--scale': text / 14,
    '--spacing': spacing[display.spacing],
    '--line': line[display.line_spacing],
    '--list-width': `${listWidth[display.list_width]}px`,
    '--column-width': display.layout === 'centred' ? `${display.column_width}px` : '100%',
    '--bubble-width': `${display.bubble_width}%`,
    '--bubble-radius': radius[display.bubble_shape],
    '--profile-width': `${display.profile_width}px`,
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
