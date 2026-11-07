Review a change to Dispatch, read-only: branch {{branch}} at {{head}} in {{worktree}}. This
pass reviews {{scope}}: `{{diff}}`. Change nothing in the worktree and write
nothing to GitHub; the one file you write is {{findings}}.

What the change is for:

{{purpose}}

What was approved is {{decisions}}: a finding that would change it goes under Out of scope.
Hold the change to {{rules}}, and read:

{{reading}}
{{earlier}}
Write the findings, most important first, each a heading with the evidence beneath it:

```
## Important: <path>:<line> <what is wrong>
How it fails, and what to change.
## Nit: <path>:<line> <what would be better>
## Pre-existing: <path>:<line> <what is wrong>
## Question: <path>:<line> <a choice to justify>
## Out of scope: <path>:<line> <what>
```

Check each against the code, and leave out what you can't show. With nothing to report, write
`No findings.` alone.
