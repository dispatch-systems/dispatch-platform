import fs from 'node:fs';
import http from 'node:http';
import type { AddressInfo } from 'node:net';
import { redact } from './transport.js';

/**
 * A stand-in for the apps' model, since an app in a home of its own has no subscription
 * signed in. It speaks just enough of Anthropic's Messages API and OpenAI's Responses and
 * Chat Completions APIs to ask for Dispatch's whoami tool once (through the app's tool
 * search when it defers its tools) and then to repeat what the tool answered. So the call
 * itself is the app's own, over its own sign-in; only the choice to make it is scripted.
 */
export async function standIn(log?: string) {
  /** What the tool answered, as each app handed it back. */
  const returned: string[] = [];
  const sse = (events: [string, unknown][]) =>
    events.map(([event, data]) => `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`).join('');
  const text = (value: unknown): string =>
    typeof value === 'string'
      ? value
      : Array.isArray(value)
        ? value.map((part) => text(part?.text ?? part?.content ?? '')).join('')
        : '';
  /** Anthropic: one tool call or one text reply, as the request's turn needs. */
  const anthropic = (body: any) => {
    const tools: string[] = (body.tools ?? []).map((tool: any) => tool.name);
    // Anywhere in the conversation: an app may add its own messages after a tool's answer.
    const results = (body.messages ?? [])
      .flatMap((message: any) => (Array.isArray(message.content) ? message.content : []))
      .filter((part: any) => part.type === 'tool_result');
    const answered = results.find((part: any) => String(part.tool_use_id).startsWith('toolu_who'));
    const searched = results.some((part: any) => String(part.tool_use_id).startsWith('toolu_find'));
    const use = answered
      ? null
      : tools.includes('mcp__dispatch__whoami') || searched
        ? { id: `toolu_who${Date.now()}`, name: 'mcp__dispatch__whoami', input: {} }
        : tools.includes('ToolSearch')
          ? {
              id: `toolu_find${Date.now()}`,
              name: 'ToolSearch',
              input: { query: 'select:mcp__dispatch__whoami' },
            }
          : null;
    if (answered) returned.push(text(answered.content));
    const reply = answered ? `Dispatch says: ${text(answered.content)}` : 'No Dispatch tool here.';
    const block = use
      ? { type: 'tool_use', id: use.id, name: use.name, input: {} }
      : { type: 'text', text: '' };
    const message = {
      id: `msg_${Date.now()}`,
      type: 'message',
      role: 'assistant',
      model: body.model,
      content: [],
      stop_reason: null,
      stop_sequence: null,
      usage: { input_tokens: 1, output_tokens: 1 },
    };
    const delta = use
      ? { type: 'input_json_delta', partial_json: JSON.stringify(use.input) }
      : { type: 'text_delta', text: reply };
    const stop = use ? 'tool_use' : 'end_turn';
    if (!body.stream)
      return {
        type: 'application/json',
        body: JSON.stringify({
          ...message,
          content: [use ? { ...block, input: use.input } : { type: 'text', text: reply }],
          stop_reason: stop,
        }),
      };
    return {
      type: 'text/event-stream',
      body: sse([
        ['message_start', { type: 'message_start', message }],
        ['content_block_start', { type: 'content_block_start', index: 0, content_block: block }],
        ['content_block_delta', { type: 'content_block_delta', index: 0, delta }],
        ['content_block_stop', { type: 'content_block_stop', index: 0 }],
        [
          'message_delta',
          {
            type: 'message_delta',
            delta: { stop_reason: stop, stop_sequence: null },
            usage: { output_tokens: 1 },
          },
        ],
        ['message_stop', { type: 'message_stop' }],
      ]),
    };
  };
  /** OpenAI Responses: the same, as Codex reads it. */
  const responses = (body: any) => {
    const flat = (body.tools ?? []).flatMap((tool: any) =>
      tool.type === 'namespace'
        ? (tool.tools ?? []).map((inner: any) => ({ namespace: tool.name, name: inner.name }))
        : [{ name: tool.name }],
    );
    // Codex groups an MCP server's tools under a namespace, `mcp__dispatch`.
    const whoami = flat.find(
      (tool: any) =>
        (tool.namespace === 'mcp__dispatch' && tool.name === 'whoami') ||
        /^mcp_+dispatch_+whoami$/.test(tool.name),
    );
    const output = (body.input ?? []).find(
      (item: any) =>
        item.type === 'function_call_output' && String(item.call_id).startsWith('call_whoami'),
    );
    if (output) returned.push(text(output.output));
    const item = output
      ? {
          type: 'message',
          role: 'assistant',
          id: `msg_${Date.now()}`,
          content: [{ type: 'output_text', text: `Dispatch says: ${text(output.output)}` }],
        }
      : whoami
        ? {
            type: 'function_call',
            id: `fc_${Date.now()}`,
            call_id: 'call_whoami',
            ...(whoami.namespace ? { namespace: whoami.namespace } : {}),
            name: whoami.name,
            arguments: '{}',
          }
        : {
            type: 'message',
            role: 'assistant',
            id: `msg_${Date.now()}`,
            content: [{ type: 'output_text', text: 'No Dispatch tool here.' }],
          };
    const response = {
      id: `resp_${Date.now()}`,
      object: 'response',
      status: 'completed',
      model: body.model,
      output: [item],
      usage: {
        input_tokens: 1,
        input_tokens_details: { cached_tokens: 0 },
        output_tokens: 1,
        output_tokens_details: { reasoning_tokens: 0 },
        total_tokens: 2,
      },
    };
    if (!body.stream) return { type: 'application/json', body: JSON.stringify(response) };
    return {
      type: 'text/event-stream',
      body: sse([
        ['response.created', { type: 'response.created', response: { ...response, output: [] } }],
        [
          'response.output_item.added',
          { type: 'response.output_item.added', output_index: 0, item },
        ],
        ['response.output_item.done', { type: 'response.output_item.done', output_index: 0, item }],
        ['response.completed', { type: 'response.completed', response }],
      ]),
    };
  };
  /** OpenAI Chat Completions: the same, as Hermes reads it from a provider of its own. */
  const completions = (body: any) => {
    const names: string[] = (body.tools ?? []).map((tool: any) => tool.function?.name);
    const direct = names.find((name) => /^mcp_+dispatch_+whoami$/.test(name ?? ''));
    // Hermes defers MCP tools: one `tool_call` tool invokes them by name.
    const whoami = direct
      ? { name: direct, arguments: '{}' }
      : names.includes('tool_call')
        ? {
            name: 'tool_call',
            arguments: JSON.stringify({
              calls: [{ name: 'mcp__dispatch__whoami', arguments: {} }],
            }),
          }
        : null;
    const result = (body.messages ?? []).find(
      (message: any) => message.role === 'tool' && message.tool_call_id === 'call_whoami',
    );
    if (result) returned.push(text(result.content));
    const call = !result && whoami !== null;
    const message = call
      ? {
          role: 'assistant',
          content: null,
          tool_calls: [{ id: 'call_whoami', type: 'function', function: whoami }],
        }
      : {
          role: 'assistant',
          content: result ? `Dispatch says: ${text(result.content)}` : 'No Dispatch tool here.',
        };
    const finish = call ? 'tool_calls' : 'stop';
    const head = { id: `chatcmpl_${Date.now()}`, created: 0, model: body.model };
    const usage = { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 };
    if (!body.stream)
      return {
        type: 'application/json',
        body: JSON.stringify({
          ...head,
          object: 'chat.completion',
          choices: [{ index: 0, message, finish_reason: finish }],
          usage,
        }),
      };
    const delta = call
      ? { role: 'assistant', tool_calls: [{ index: 0, ...message.tool_calls![0] }] }
      : { role: 'assistant', content: message.content };
    const chunk = (choice: object, extra = {}) =>
      `data: ${JSON.stringify({ ...head, object: 'chat.completion.chunk', choices: [{ index: 0, ...choice }], ...extra })}\n\n`;
    return {
      type: 'text/event-stream',
      body:
        chunk({ delta, finish_reason: null }) +
        chunk({ delta: {}, finish_reason: finish }, { usage }) +
        'data: [DONE]\n\n',
    };
  };
  const server = http.createServer((request, response) => {
    let raw = '';
    request.on('data', (chunk) => (raw += chunk));
    request.on('end', () => {
      let body: any = {};
      try {
        body = JSON.parse(raw || '{}');
      } catch {}
      if (log)
        fs.appendFileSync(log, `${JSON.stringify({ path: request.url, body: redact(raw) })}\n`);
      const route = new URL(request.url ?? '/', 'http://x').pathname;
      const answer =
        route === '/v1/messages'
          ? anthropic(body)
          : route === '/v1/messages/count_tokens'
            ? { type: 'application/json', body: '{"input_tokens":1}' }
            : route === '/v1/responses'
              ? responses(body)
              : route === '/v1/chat/completions'
                ? completions(body)
                : route === '/v1/models'
                  ? {
                      type: 'application/json',
                      body: '{"object":"list","data":[{"id":"stand-in","object":"model"}]}',
                    }
                  : null;
      if (!answer) {
        response.writeHead(404, { 'content-type': 'application/json' });
        response.end('{"error":{"type":"not_found_error","message":"not here"}}');
        return;
      }
      response.writeHead(200, { 'content-type': answer.type });
      response.end(answer.body);
    });
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address() as AddressInfo;
  return { url: `http://127.0.0.1:${port}`, returned, close: () => server.close() };
}
