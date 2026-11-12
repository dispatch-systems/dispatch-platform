import { peopleFeatures as publicPeopleFeatures } from '../../../manifest/api/generated/people-features.js';
import type { LucideIcon } from 'lucide-react';
import type { Feature } from '../../../tenancy/api/index.js';

export type PeopleKindInfo = {
  id: string;
  label: string;
  order: number;
  unit: string | null;
  icon: LucideIcon;
};
export type PeopleSourceInfo = { id: string; label: string; order: number };
export type PeopleDescriptors = {
  peopleKinds?: readonly PeopleKindInfo[];
  peopleSources?: readonly PeopleSourceInfo[];
  peopleFeatures?: readonly Feature[];
};
export let peopleFeatures: readonly Feature[] = publicPeopleFeatures;
export let privatePeopleKinds: readonly PeopleKindInfo[] = [];
export let privatePeopleSources: readonly PeopleSourceInfo[] = [];

/** These descriptors come from modules admitted for this DSP, never the public entry. */
export function installCustomPeople(modules: readonly PeopleDescriptors[]) {
  peopleFeatures = [
    ...publicPeopleFeatures,
    ...modules.flatMap((module) => module.peopleFeatures ?? []),
  ];
  privatePeopleKinds = modules.flatMap((module) => module.peopleKinds ?? []);
  privatePeopleSources = modules.flatMap((module) => module.peopleSources ?? []);
}

export function peopleOrder<T extends { id: string; order: number }>(items: readonly T[]): T[] {
  return [...new Map(items.map((item) => [item.id, item])).values()].sort(
    (a, b) => a.order - b.order || a.id.localeCompare(b.id),
  );
}
