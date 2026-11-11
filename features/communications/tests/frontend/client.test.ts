import assert from 'node:assert/strict';
import test from 'node:test';
import type { ConversationWindow } from '../../api/index.js';
import { ApiError } from '../../../../core/shell/frontend/runtime/api.js';
import { dataCache } from '../../../../core/shell/frontend/runtime/data-cache.js';
import {
  getConversationWindow,
  quickRepliesUrl,
  saveQuickReplies,
  textAttachments,
} from '../../api/client.js';

/** A browser's request, as `upload` sends one, which the server refuses as `answer` says. */
function refusing(answer: { status: number; body: unknown; requestId: string }) {
  return class {
    status = 0;
    responseText = '';
    upload = { onprogress: undefined };
    onload?: () => void;
    open() {}
    setRequestHeader() {}
    getResponseHeader(name: string) {
      return name === 'x-request-id' ? answer.requestId : null;
    }
    send() {
      this.status = answer.status;
      this.responseText = JSON.stringify(answer.body);
      queueMicrotask(() => this.onload?.());
    }
  };
}

test('a picture for a text the server refuses is worded as Communications words that refusal', async () => {
  globalThis.XMLHttpRequest = refusing({
    status: 415,
    body: { error: 'communications_photo_kind' },
    requestId: 'req_1',
  }) as unknown as typeof XMLHttpRequest;
  const photo = new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], 'door.png', {
    type: 'image/png',
  });
  await assert.rejects(textAttachments.add(photo), (error) => {
    assert.ok(error instanceof ApiError);
    assert.deepEqual(
      [error.message, error.code, error.status, error.requestId],
      ['Photos must be PNG, JPEG or GIF.', 'communications_photo_kind', 415, 'req_1'],
    );
    return true;
  });
});

test('quick replies saved show as saved at once, on the revision the save answered, while the list is read again', async (t) => {
  dataCache.put(quickRepliesUrl, { replies: ['On my way'], revision: 1 });
  const saved = { replies: ['On my way', 'Call dispatch when you can'], revision: 2 };
  t.mock.method(globalThis, 'fetch', async () => new Response(JSON.stringify(saved)));
  await saveQuickReplies(saved.replies, 1);
  assert.deepEqual(dataCache.peek(quickRepliesUrl).data, saved);
});

test('window reads preserve opaque positions, zero leading, priority and cancellation without caching', async (t) => {
  const controller = new AbortController();
  const answer: ConversationWindow = {
    conversation: {
      id: 'tc_id',
      kind: 'number',
      teamId: null,
      number: '+12065550101',
      driverId: null,
      contact: false,
      added: false,
      blocked: false,
      with: { userId: null, name: 'Test', code: null, role: null, title: null, photo: null },
      state: 'responded',
      waitingSince: null,
      lastAt: null,
      last: null,
      matched: [],
      groupId: null,
    },
    messages: [],
    latest: [],
    context: { previous: null, next: null, sentBefore: null },
    latestContext: { previous: null, next: null, sentBefore: null },
    latestWorkdays: [],
    range: {
      revision: '42',
      first: null,
      last: null,
      hasBefore: false,
      hasAfter: false,
      before: null,
      after: null,
      refresh: 'opaque.refresh',
      rebased: true,
    },
    today: null,
    working: false,
    elsewhere: [],
    workdays: [],
    limits: { text: 10000, photos: 10, largest: 20 * 1024 * 1024, kinds: [] },
  };
  const asked: { url: string; init?: RequestInit }[] = [];
  t.mock.method(globalThis, 'fetch', async (url: string, init?: RequestInit) => {
    asked.push({ url, init });
    return new Response(JSON.stringify(answer));
  });
  const anchor = '1700000000000:text-000100';
  assert.deepEqual(
    await getConversationWindow('tc_id', { anchor, leading: 0 }, 'low', controller.signal),
    answer,
  );
  const first = asked[0]!;
  const parsed = new URL(first.url, 'https://dispatch.test');
  assert.equal(parsed.pathname, '/api/dsp/communications/conversations/tc_id/window');
  assert.equal(parsed.searchParams.get('anchor'), anchor);
  assert.equal(parsed.searchParams.get('leading'), '0');
  assert.equal(first.init?.signal?.aborted, false);
  controller.abort();
  assert.equal(first.init?.signal?.aborted, true);
  assert.equal((first.init as RequestInit & { priority?: string })?.priority, 'low');
  const cursor = 'opaque.with_-bytes';
  await getConversationWindow('tc_id', { cursor });
  assert.equal(new URL(asked[1]!.url, 'https://dispatch.test').searchParams.get('cursor'), cursor);
  assert.equal(new URL(asked[1]!.url, 'https://dispatch.test').searchParams.has('anchor'), false);
  await getConversationWindow('tc_id');
  assert.equal(new URL(asked[2]!.url, 'https://dispatch.test').search, '');
  await getConversationWindow('tc_id', { origins: 1 });
  assert.equal(new URL(asked[3]!.url, 'https://dispatch.test').searchParams.get('origins'), '1');
  await getConversationWindow('tc_id', { cursor, origins: 1 });
  const exact = new URL(asked[4]!.url, 'https://dispatch.test');
  assert.equal(exact.searchParams.get('cursor'), cursor);
  assert.equal(exact.searchParams.get('origins'), '1');
  assert.equal(exact.searchParams.has('anchor'), false);
  assert.equal(dataCache.peek(parsed.pathname).data, undefined);
});
