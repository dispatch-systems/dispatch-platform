import { sectionIdentities } from './generated/identities.js';

type Identity = Readonly<{ id: string; aliases: readonly string[]; owner: string | null }>;

/** A bundle's one local identity for the section, without changing tenant eligibility. */
export const sectionResolver =
  (identities: readonly Identity[]) =>
  (id: string, code?: string | null): string =>
    identities.find(
      (family) =>
        (family.owner === null || family.owner === code) &&
        (family.id === id || family.aliases.includes(id)),
    )?.id ?? id;

export const sectionId = sectionResolver(sectionIdentities);
