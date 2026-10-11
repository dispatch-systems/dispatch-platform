# AI Support worker

The dev host runs one private Rust supervisor for tickets from **both dev and production**.
Both origins use the same checked-in analyst, builder and reviewer profile manifests, model,
skills and exact tool inventories. Only ticket/origin/source/approval context changes. Hermes
uses OpenAI Codex OAuth, `gpt-6.1-sol`, `xhigh`, standard speed. It calls the installed `AIAgent`
core directly, avoiding the personal gateway and the updating CLI wrapper.

`SUPPORT_WORKER_CONFIG` points to a private JSON file:

```json
{
  "origins": [
    {
      "environment": "dev",
      "url": "https://dispatchdev.example.com",
      "tokenFile": "/private/dev-support.token"
    },
    {
      "environment": "production",
      "url": "https://dispatch.example.com",
      "tokenFile": "/private/production-support.token"
    }
  ],
  "workspaceRoot": "/opt/dispatch/workspace",
  "runtimeRoot": "/var/lib/dispatch/support-worker",
  "hermesSource": "/opt/hermes/hermes-agent",
  "hermesPython": "/opt/hermes/tools/managed-python/bin/python3",
  "repository": "dispatch-systems/dispatch-platform",
  "collectionBridge": "/installed/services/support-worker/support-collection.py",
  "node": "/opt/node/bin/node"
}
```

Installed runtime resources are selected by `SUPPORT_WORKER_RESOURCES`. Profile instances live
under `~/.hermes/profiles/dispatch-support-<environment>-<ticket>-<role>` so Hermes's supported
root-auth fallback refreshes the single shared OAuth store. Refresh tokens are never copied.
Profiles carry private transcripts, browser evidence and isolated sessions; no personal memory,
skill management, cron, delegation, broad terminal or dynamic MCP expansion is enabled.

The platform owns job truth. The worker claims through `/api/v1/support/worker`, renews a
90-second lease every 25 seconds and persists phase/head/PR/failure checkpoints. Restarting
resumes pending jobs; losing the lease or approved proposal cancels the entire execution group.
One heavy job runs at a time and origins rotate. This is resource scheduling, with no usage,
token, runtime, daily or correction-attempt quota.

The analyst first routes the public report/conversation with zero tools and no source directory,
browser grant, diagnostics or preview. Genuine tests, greetings and thanks receive an
acknowledgement; supported one-off questions receive an answer. Source questions select the
source-only stage, which cannot call a browser. UI questions and real vague reports select
read-only observation when useful. These named stages share the existing analyst manifest and
the same Codex subscription/model configuration across both environments.

Useful investigation reads the exact submitted runtime commit and may inspect the actual
application with a lease-bound reporter read-only session. A build/isolated preview runs only
after the analyst requests a specific necessary mutation/reproduction, including why the other
evidence is insufficient. Unreproduced identified bugs still receive proposals with uncertainty.
An unclear bug/feature outcome requires actual best-effort investigation and useful clarification
questions; reporter replies resume tool-free routing. Skipped or unused browser work is recorded
as such and never counted as passing evidence. Owner questions use prior private context and
select new tools only when needed. Public analyst payloads exclude private owner messages,
proposals, evidence, attachments and intake findings before any model call.

Owner approval names one immutable proposal and its criteria. The builder uses a separate
current-main worktree; an independent reviewer exercises its committed head in a fresh real
Chromium browser. Host-recorded assertions and private screenshots must cover every criterion.
Claimed success without a real browser witness cannot publish a PR.

Raw commands run in bubblewrap with only the assigned source, package/compiler caches and
temporary storage. Analyst/reviewer source is read-only; Git administration is read-only even
for builders. Host files, SSH, OAuth, support tokens, provider credentials and GitHub authority
are unavailable inside commands. The browser has no arbitrary JavaScript tool, refuses other
origins and WebSockets, and blocks production mutations except the existing read-only DSP
session lookup. Native screenshots use Hermes's vision envelope for the selected main model.

Collection changes run the changed branch's collector verifier using the configured Dev
verification account. Credentials stay outside Hermes, verification state is private and
regular account runs share a provider lock. Authentication/MFA failure waits rather than passes.

Changed build and check scripts also run in bubblewrap, with no host network, a projected Git
index and refs, and isolated build caches. Preview servers see only their artifact and private
fixture or verified collection state. A private Unix relay exposes a loopback browser origin;
the server cannot reach host or production services. Browser tooling is loaded from the trusted
installed Dev dependency tree, never from agent-modifiable dependencies.

The trusted publisher rechecks approval, uses the existing `dispatchdev` PR workflow, stores
the accepted PR/head in the platform, then registers correlation with the Dispatch bot. No
`ai-review` label is added. GitHub admission and the existing merge queue remain mandatory.
Failures and conflicts return to implementation and independent verification until GitHub
confirms the merge. The ticket then waits for a separately authorized production release;
production source ancestry and read-only browser evidence determine resolution.

Focused checks:

```sh
cargo test -p dispatch-support-worker --test authority
python3 ops/tests/support_tool_policy_test.py
python3 ops/tests/support_host_sandbox_test.py
```
