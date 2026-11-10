Review a change to Dispatch for security, read-only: branch {{branch}} at {{head}}, checked out for you
in {{checkout}}. Read it there, not in the change's own worktree, which moves on while you
read. Nothing is installed or built there, so build and run nothing: show a finding from the
code. This pass reviews {{scope}}: `{{diff}}`. Change nothing and write nothing to GitHub; the
one file you write is {{findings}}.

What the change is for:

{{purpose}}

What was approved is {{decisions}}: a finding that would change it goes under Out of scope.
Look only for security defects the change introduces, holding it to the rules in {{rules}} that
cover its paths, above all those on logs and request events, credentials, sessions and MFA,
route access, DSP isolation and migrations. Also look for:

- injection: SQL built from input, shell commands, paths that escape their folder;
- cross-site scripting and HTML or URLs built from untrusted text;
- authorization: a route, tool or job reading or changing more than its caller may, or not
  checking again after a wait;
- secrets, tokens, cookies or personal data reaching source, logs, errors or the client;
- input trusted without validation, at every boundary it crosses.
{{earlier}}
Write the findings, most important first, each a heading with the evidence beneath it:

```
## Important: <path>:<line> <what is wrong>
How it is exploited or leaks, and what to change.
## Nit: <path>:<line> <a hardening worth making>
## Pre-existing: <path>:<line> <what is wrong>
## Question: <path>:<line> <a choice to justify>
## Out of scope: <path>:<line> <what>
```

Check each against the code, and leave out what you can't show. With nothing to report, write
`No findings.` alone.
