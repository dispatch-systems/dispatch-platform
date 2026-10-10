# MCP

How an outside agent connects to Dispatch, and the tools it uses there, all in this folder: its
own crate, `dispatch-mcp`, above the features (`app → mcp → features → collectors → core`).
Core never names it: the app installs it as the registry's agents' piece (`backend/piece.rs`),
through which core hands it its routes, migrations, upkeep and the requests agents send.

| Folder        | Holds                                                                               |
| ------------- | ----------------------------------------------------------------------------------- |
| `backend/`    | Keys, Sign in with Dispatch (`oauth/`), the server, its tools (`tools/`), the log   |
| `api/`        | Its routes and API types, and the Agents page's client                              |
| `frontend/`   | The Agents page and Connect an app, and its words in the platform owner's audit log |
| `migrations/` | Its tables in the platform's database, numbered with core's                         |
| `tests/`      | Its own tests; the app's `tests/` hold those that need the whole product            |

It reads people and DSPs through core's functions, never core's tables.

## Connecting

Agent keys and connected apps, Sign in with Dispatch (`oauth`), usage limits and the Activity
log, and the MCP server at `/api/v1/mcp`. A key or app belongs to the platform owner who made or
approved it, reaches the DSPs it was given, and stops at once when revoked, when it expires or
when its owner stops being an active platform owner. Every call checks the key again before it
answers. `GET /api/v1/whoami` answers what the `whoami` tool does, over REST.

## Tools

Every tool lives in `tools/`, a file each, listed in `tools::TOOLS`, with its test in
`tests/backend/tools/`; what they are made of is `backend/toolbox/`.
`dispatchdev new tool <name>` makes one that works as written, with its test, and lists it; its
`--features`, `--scope`, `--actions`, `--changes` and `--changing` set what the list below
describes. A tool is a type implementing `toolbox::Tool`:

- `NAME`, `TITLE` and `DESCRIPTION`: what agents call it and read about it.
- `SCOPE`: one DSP (`Dsp`, the default), several (`Dsps`), or the connection itself
  (`Connection`). A call names its DSP with `dsp`, or its DSPs with `dsps`; the server reads
  them, so the tool takes neither itself.
- `FEATURES`: the features, or their parts, it needs, by their switches, as
  `&["timecard", "routes"]`. It runs at a DSP only where every one is on. One it uses only where
  it is on, it asks about with `cx.has(dsp, "dvic")`.
- `Input` and `Output`: what it takes and answers, Rust types whose JSON Schema the server
  publishes (`schemars::JsonSchema`). `Input` refuses fields it doesn't name. It can be a choice
  of actions, an enum with `#[serde(tag = "action")]`, each with its own arguments, so one tool
  can do several things.
- `EFFECT` and `effect(&input)`: the most it does, `Reads` or `Changes`, and what each call
  does, so a tool of several actions says which of them change something.
- `call(cx, input)`: async code that answers. It reads with `cx.read(|db| …)` and changes
  something with `cx.write(|w| …)`, recording the change with `w.audit` as made by the platform
  owner through the agent; it may wait or call another service in between. Only a call it said
  changes something may write. It answers with its data, and words and pictures beside it where
  they help (`Reply::new(data).text(…).image(…)`), or refuses with a code and a message that
  says what to fix and lists the choices.

A tool never says who may use it: the connection does (`Grants`). For each tool the platform
owner chooses Off, Read, or for a tool that can change something, Read and change, when making a
key or approving an app, and changes it on the Agents page whenever they like; and whether tools
added later come, to read. Nothing added later changes something until it is allowed to.
`get_profile` and `whoami`, about the connection itself, only read and are always allowed.
Before any tool runs, the server checks, the same for every tool:

- the key or app still stands, and may do what the call does with the tool, as it stands now;
- the DSPs the call names are ones the connection reaches; a call may leave `dsp` out when the
  connection reaches one DSP, and `dsps` out for every one it reaches with the tool's features
  on;
- the tool's features are switched on at those DSPs. Hidden from a DSP's members, they still
  run for agents.

Each `cx.read` and `cx.write` checks the connection again, as it stands then. The server lists
only the tools a connection may use where at least one DSP it reaches has their features on,
marking as read-only a tool the connection may only read with, and records every call in the
Activity log. The registry refuses, when the server starts, a tool with another's name, one that
needs a feature there isn't, one that takes fields it doesn't name or its own `dsp` or `dsps`,
one about the connection that changes something or needs a feature, and one that answers
anything but an object.

Tool names, arguments and answers are what agents rely on: app/tests/backend/snapshots/agent-tools.json
holds every tool's, so a change to one shows in review. `testing::install` installs core's test
registry with the MCP, `testing::agent` makes a key, `testing::serving` gives the server's state
over a store, and `testing::call_tool` calls a tool as the server does; `npm run test:mcp` runs
every test of the MCP's.

## Storage an older release reads

`agent_key_tools` keeps each key's choices, a row a tool: whether it may use the tool
(`allowed`), and whether it may also change something with it (`changes`); and
`agent_keys.all_tools` whether tools added later come, to read. The tables keep columns an older release reads
and writes: what a key reads (`areas`,
`bypass`, `locations`, `tools`, `agent_key_dsp_reads`) and whether a call bypassed a feature
(`bypassed`). This release writes a new key's as reading nothing, and reads none of them.
