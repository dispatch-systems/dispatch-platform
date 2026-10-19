import type { ComponentType } from 'react';
import type { SettingsField, SettingsValue } from '../../../../core/settings/api/index.js';

// What a kind of field is to the page: the control that edits it, and what a value of it must be.

/** A field of one kind. */
export type Field<K extends SettingsField['kind']> = Extract<SettingsField, { kind: K }>;
/** What a field's control is given: the field, its element's id, and what it holds. */
export type FieldProps<K extends SettingsField['kind']> = {
  field: Field<K>;
  id: string;
  /** What it holds as edited: its value, or for a number its text as typed. */
  value: SettingsValue;
  /** Its value as saved. */
  before: SettingsValue;
  change: (value: SettingsValue) => void;
};
/** A kind of field: its control, and what it holds once saved and whether that may be. */
export type FieldKind<K extends SettingsField['kind']> = {
  Control: ComponentType<FieldProps<K>>;
  /** Whether its control names itself, as a switch does; else the row's label names it. */
  labelled?: (field: Field<K>) => boolean;
  /** Not a value of the section's, saved with it: a control that saves itself. */
  apart?: true;
  /** Whether what it holds may be saved, given the value it had; omitted, anything it holds. */
  valid?: (field: Field<K>, value: SettingsValue, before: SettingsValue) => boolean;
  /** The value to save for what it holds; omitted, what it holds. */
  saved?: (value: SettingsValue) => SettingsValue;
};
