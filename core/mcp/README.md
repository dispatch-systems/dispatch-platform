# MCP

How an outside agent connects to Dispatch: agent keys and connected apps, Sign in with
Dispatch (`oauth`), usage limits and the Activity log, and the MCP server at `/api/v1/mcp`.
A key or app belongs to the platform owner who made or approved it, reaches the DSPs it was
given, and stops at once when revoked, when it expires or when its owner stops being an active
platform owner. Every call checks the key again before it answers.

The server offers the tools that tell an agent about its own connection: `get_profile`, the
stable profile it is signed in as, and `whoami`, its key, the time and the DSPs it reaches with
each one's date today. `GET /api/v1/whoami` answers the same over REST. Tool names, paths and
answers are what agents rely on, and nothing an agent does appears in a DSP's activity log.

The tables keep columns an older release reads and writes: what a key reads (`areas`,
`bypass`, `locations`, `tools`, `agent_key_dsp_reads`) and whether a call bypassed a feature
(`bypassed`). This release writes a new key's as reading nothing, and reads none of them.
