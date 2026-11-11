import { z } from 'zod/mini';
import { text, type Replies } from '../../foundation/api/runtime.js';
import { sectionId } from './identities.js';

// Normalize identity fields before every hook, direct read or raw prefetch can cache them.
// Loose objects retain the values, revision and owner-specific field descriptions unchanged.
const section = z.looseObject({ id: text, page: text });
const page = z.looseObject({ id: text });
const listing = z.looseObject({ pages: z.array(page), sections: z.array(section) });
const saved = z.union([
  z.looseObject({ section, reopen: z.boolean() }),
  z.object({ warning: text }),
]);

export const replies: Replies = (route, method, context) => {
  const normalize = <T extends { id: string }>(value: T): T => ({
    ...value,
    id: sectionId(value.id, context?.dsp?.code),
  });
  if (route === '/api/dsp/settings' && method === 'GET')
    return z.pipe(
      listing,
      z.transform((value) => {
        const sections = value.sections.map(normalize);
        if (new Set(sections.map((section) => section.id)).size !== sections.length)
          throw new Error('invalid_api_response');
        return { ...value, sections };
      }),
    );
  if (route.startsWith('/api/dsp/settings/') && method === 'POST')
    return z.pipe(
      saved,
      z.transform((value) =>
        'section' in value ? { ...value, section: normalize(value.section) } : value,
      ),
    );
  return undefined;
};
