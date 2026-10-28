import assert from 'node:assert/strict';
import test from 'node:test';
import { ApiError } from '../../../../core/shell/frontend/runtime/api.js';
import { textAttachments } from '../../api/client.js';

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
