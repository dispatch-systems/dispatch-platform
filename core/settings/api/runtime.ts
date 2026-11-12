import { z } from 'zod/mini';
import { text, type Replies } from '../../foundation/api/runtime.js';
import { sectionId } from './identities.js';

// Normalize identity fields before every hook, direct read or raw prefetch can cache them.
// Loose objects retain the values, revision and owner-specific field descriptions unchanged.
export const replies: Replies = (route, method, context) => {
  const read = method === 'GET';
  if (read ? route !== '/api/dsp/settings' : !route.startsWith('/api/dsp/settings/'))
    return undefined;

  const section = z.looseObject({
    id: text.check((parsed) => {
      parsed.value = sectionId(parsed.value, context?.dsp?.code);
    }),
    page: text,
  });
  if (read)
    return z
      .looseObject({ pages: z.array(z.looseObject({ id: text })), sections: z.array(section) })
      .check(({ value: { sections } }) => {
        if (new Set(sections.map((section) => section.id)).size !== sections.length)
          throw new Error('invalid_api_response');
      });
  return z.union([z.looseObject({ section, reopen: z.boolean() }), z.object({ warning: text })]);
};
