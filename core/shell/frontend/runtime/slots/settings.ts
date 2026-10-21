import type { ReactNode } from 'react';
import type { DspView, SessionView } from '../../../../accounts/api/index.js';
import { manifests } from './installed.js';

// The Settings slot: the sections of a DSP's Settings an owner draws itself. Every section is
// declared in its owner's backend manifest, and the Settings page draws each from what the
// server says of it; one declared `custom` is drawn by the component its owner registers here
// under the section's id, for what isn't a value, as a sign-in flow or a list of channels.

/** What a Settings section is drawn for: the person, and inside a DSP its view. */
export type SettingsContext = { session: SessionView; view?: DspView };
/**
 * What a custom section is drawn with: its context; `refresh`, which reads Settings again; and in
 * a DSP's Settings, for the Collectors page, the declared sections that go with a collection,
 * which it draws with that collection.
 */
export type CustomSectionContext = SettingsContext & {
  refresh: () => void;
  /** The sections that go with the collection `schedule` names, drawn as Settings draws its own. */
  collectionSections?: (schedule: string) => ReactNode;
  /** The collection of the section the address points out, which the page shows. */
  pointed?: string;
};
/**
 * The component of a section its owner's backend declares `custom`. The page puts the space
 * between its sections: what it draws keeps none above it of its own.
 */
export type CustomSection = {
  /** The section it draws, as declared: `<owner>.<what>`. It never changes. */
  id: string;
  /** Loads its code; the page opens once the code of its open page's sections is in. */
  load: () => Promise<unknown>;
  render: (context: CustomSectionContext) => ReactNode;
  /**
   * Whether it has anything to show the person, beyond the permission the server checked;
   * omitted, it always has. A page with nothing to show is left out of Settings.
   */
  visible?: (view: DspView) => boolean;
  /** The reads to warm, for the admitted view, while its page is about to open. */
  prefetch?: (view: DspView) => string[];
};

declare module './installed.js' {
  interface FrontendFeature {
    /** The components of its custom sections of a DSP's Settings. */
    settingsSections?: readonly CustomSection[];
  }
}

/** The component an owner registers for a custom section, by the section's id. */
export const settingsSection = (id: string) =>
  manifests()
    .flatMap((feature) => feature.settingsSections ?? [])
    .find((section) => section.id === id);

/** The read a DSP's Settings is drawn from: its pages, and the sections on them the member sees. */
export const settingsRead = '/api/dsp/settings';
