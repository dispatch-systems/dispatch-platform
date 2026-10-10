# MCP

How an outside agent connects to Dispatch, and the tools it uses there.

## Connecting

Agent keys and connected apps, Sign in with Dispatch (`oauth`), usage limits and the Activity
log, and the MCP server at `/api/v1/mcp`. A key or app belongs to the platform owner who made or
approved it, reaches the DSPs it was given, and stops at once when revoked, when it expires or
when its owner stops being an active platform owner. Every call checks the key again before it
answers. `GET /api/v1/whoami` answers what the `whoami` tool does, over REST.

## Tools

A tool is a type of its own, in a file of its own, implementing `tools::Tool`: its name, title
and description, what it takes and answers as Rust types whose JSON Schema the server publishes
(`schemars::JsonSchema`), whether it reads or changes something (`EFFECT`), whether it is about
one DSP or the connection (`SCOPE`), and the part of its feature it belongs to, if any (`PART`).
A feature lists its tools in its manifest's `tools`, from its `mcp/`; core lists its own,
`get_profile` and `whoami`, in `tools/connection.rs`.

A tool never says who may use it: the connection does (`Grants`). The platform owner chooses
when making a key or approving an app, and changes on the Agents page whenever they like, the
tools it may use, each a switch, and whether tools added later that only read come with it. A
tool added later that changes something waits until it is switched on. `get_profile` and
`whoami`, about the connection itself, are always allowed. Before any tool runs, core checks,
the same for every tool:

- the key or app still stands, and may use the tool, as it stands now;
- the DSP the call names with `dsp`, by name or ID, is one the connection reaches; a call may
  leave it out when the connection reaches one DSP;
- the tool's feature, or its part, is switched on at that DSP. Hidden from the DSP's members,
  it still runs for agents.

The tool is then handed a `Cx`: the store, the connection and the checked DSP. A tool that reads
runs under the shared lock, one that changes something under the exclusive one, and records its
change with `cx.audit`, as made by the platform owner through the agent. It answers, or refuses
with a code and a message that says what to fix and lists the choices. The server lists only
the tools a connection may use where at least one DSP it reaches has them on, and records every
call in the Activity log. The registry refuses a tool with another's name, one that takes fields
it doesn't name or its own `dsp`, and one that answers anything but an object, when the server
starts.

Tool names, arguments and answers are what agents rely on: app/tests/backend/snapshots/agent-tools.json
holds every tool's, so a change to one shows in review. `testing::agent` makes a key and
`testing::call_tool` calls a tool as the server does, for a tool's own tests in its feature's
`tests/backend/mcp/`.

## Storage an older release reads

`agent_key_tools` keeps each key's choices, a row a tool, and `agent_keys.all_tools` whether
tools added later that only read come with it. The tables keep columns an older release reads
and writes: what a key reads (`areas`,
`bypass`, `locations`, `tools`, `agent_key_dsp_reads`) and whether a call bypassed a feature
(`bypassed`). This release writes a new key's as reading nothing, and reads none of them.
