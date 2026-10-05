# MCP

The MCP server and the agent API under `/api/v1/`, with its OpenAPI document and the Agent
Skill, all built from one catalog so they can't drift apart; agent keys and connected apps,
Sign in with Dispatch, usage limits and activity. Features bring their endpoints, facts, read
toggles and the skill's example questions in their `mcp/`. Tool names, paths and answers are what agents rely on, and nothing an
agent does appears in a DSP's activity log.
