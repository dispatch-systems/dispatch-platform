import { useId } from 'react';
import type { AgentKey, AgentKeyRequest, AgentTool } from '../../api/index.js';
import { toolGroups, toolsText } from './agents.js';

/** What a key or app may use. */
type Choices = Pick<AgentKeyRequest, 'allTools' | 'tools'>;

/**
 * What a key or app may use: a switch for each tool, under the feature it belongs to, those that
 * make changes marked, and whether tools added later that only read come as they are added. A
 * tool that makes changes always waits until it is switched on here.
 */
export function ToolChoices({
  tools,
  form,
  set,
}: {
  tools: AgentTool[];
  form: Choices;
  set: (change: Partial<Choices>) => void;
}) {
  const id = useId();
  const toggle = (name: string, on: boolean) =>
    set({ tools: on ? [...form.tools, name] : form.tools.filter((tool) => tool !== name) });
  return (
    <fieldset aria-describedby={`${id}-hint`}>
      <legend>Tools</legend>
      <p className="agents-hint" id={`${id}-hint`}>
        {tools.length
          ? 'What it can use at the DSPs it reaches, where the tool’s feature is on.'
          : 'No tools yet.'}
      </p>
      {tools.length > 0 && (
        <div className="agents-tools">
          {toolGroups(tools).map(([label, listed], index) => (
            <div
              key={label}
              className="agents-tools-group"
              role="group"
              aria-labelledby={`${id}-group-${index}`}
            >
              <header id={`${id}-group-${index}`}>{label}</header>
              {listed.map((tool) => (
                <label key={tool.name} className="agents-tool">
                  <span>
                    <span id={`${id}-${tool.name}`}>{tool.title}</span>
                    {tool.changes && <span className="agents-changes">Makes changes</span>}
                    <small id={`${id}-${tool.name}-hint`}>{tool.description}</small>
                  </span>
                  <input
                    type="checkbox"
                    role="switch"
                    aria-labelledby={`${id}-${tool.name}`}
                    aria-describedby={`${id}-${tool.name}-hint`}
                    checked={form.tools.includes(tool.name)}
                    onChange={(event) => toggle(tool.name, event.target.checked)}
                  />
                </label>
              ))}
            </div>
          ))}
        </div>
      )}
      <label className="agents-tool agents-new-tools">
        <span>
          <span id={`${id}-new`}>New tools that only read</span>
          <small id={`${id}-new-hint`}>
            Allowed as they are added. A new tool that makes changes waits until you switch it on.
          </small>
        </span>
        <input
          type="checkbox"
          role="switch"
          aria-labelledby={`${id}-new`}
          aria-describedby={`${id}-new-hint`}
          checked={form.allTools}
          onChange={(event) => set({ allTools: event.target.checked })}
        />
      </label>
    </fieldset>
  );
}

/** What a key or app may use, as its row in a table says it. */
export function ToolsCell({
  agentKey,
  tools,
}: {
  agentKey: Pick<AgentKey, 'allTools' | 'tools'>;
  tools: AgentTool[];
}) {
  const { count, changes } = toolsText(agentKey, tools);
  return (
    <>
      {count}
      {changes > 0 && <small className="agents-changes">Makes changes</small>}
    </>
  );
}
