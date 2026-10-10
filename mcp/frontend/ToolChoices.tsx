import { useId } from 'react';
import type { AgentKey, AgentKeyRequest, AgentTool, ToolLevel } from '../api/index.js';
import { toolGroups, toolLevels, toolsText, withLevel } from './agents.js';

/** What a key or app may use. */
type Choices = Pick<AgentKeyRequest, 'allTools' | 'tools'>;

/**
 * What a key or app may do with each tool, under the features it needs: Off, Read, or for a
 * tool that can change something, Read and change; and whether tools added later come, to
 * read, as they are added. Nothing added later changes something until it is allowed to here.
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
  const choose = (name: string, level: ToolLevel) =>
    set({ tools: withLevel(form.tools, name, level) });
  return (
    <fieldset aria-describedby={`${id}-hint`}>
      <legend>Tools</legend>
      <p className="agents-hint" id={`${id}-hint`}>
        {tools.length
          ? 'What it can do with each tool at the DSPs it reaches, where the tool’s features are on.'
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
                <div key={tool.name} className="agents-tool">
                  <span>
                    <span id={`${id}-${tool.name}`}>{tool.title}</span>
                    {tool.changes && <span className="agents-changes">Can make changes</span>}
                    <small id={`${id}-${tool.name}-hint`}>{tool.description}</small>
                  </span>
                  <span
                    className="agents-levels"
                    role="radiogroup"
                    aria-labelledby={`${id}-${tool.name}`}
                    aria-describedby={`${id}-${tool.name}-hint`}
                  >
                    {toolLevels(tool).map(([level, text]) => (
                      <label key={level} className="agents-level">
                        <input
                          type="radio"
                          name={`${id}-${tool.name}`}
                          value={level}
                          checked={(form.tools[tool.name] ?? 'off') === level}
                          onChange={() => choose(tool.name, level)}
                        />
                        <span>{text}</span>
                      </label>
                    ))}
                  </span>
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
      <label className="agents-tool agents-new-tools">
        <span>
          <span id={`${id}-new`}>New tools, to read</span>
          <small id={`${id}-new-hint`}>
            They can read as they are added. Nothing added later makes changes until you allow it.
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
