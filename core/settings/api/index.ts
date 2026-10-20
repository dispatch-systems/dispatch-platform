import type { SettingsSection } from './generated/SettingsSection';

// Settings' API: a DSP's settings as its Settings page reads and saves them.
export type { SettingsAnswer } from './generated/SettingsAnswer';
export type { SettingsField } from './generated/SettingsField';
export type { SettingsGroup } from './generated/SettingsGroup';
export type { SettingsKind } from './generated/SettingsKind';
export type { SettingsOption } from './generated/SettingsOption';
export type { SettingsPage } from './generated/SettingsPage';
export type { SettingsPreset } from './generated/SettingsPreset';
export type { SettingsSave } from './generated/SettingsSave';
export type { SettingsSaved } from './generated/SettingsSaved';
export type { SettingsScheduleChoice } from './generated/SettingsScheduleChoice';
export type { SettingsScheduleOption } from './generated/SettingsScheduleOption';
export type { SettingsScope } from './generated/SettingsScope';
export type { SettingsStart } from './generated/SettingsStart';
export type { SettingsSection };
/** A field's value, as a section holds it and a save sends it. */
export type SettingsValue = SettingsSection['values'][string];
