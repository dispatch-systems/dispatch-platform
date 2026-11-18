#!/usr/bin/env python3
"""The issue a failing nightly audit opens. `security.yml`'s scheduled run calls this after its
audit job, with that job's result: a failure opens one issue labelled `main-advisory`, naming
the run and what the audit denied, or comments on the one already open; a pass closes it. A
thread that finds `main` failing an audit looks there first: `gh issue list -l main-advisory`."""
import json
import os
import subprocess

LABEL = "main-advisory"
TITLE = "The nightly dependency audit fails"


def commands(result, open_issues, denied, run):
    """The `gh issue` commands for the audit job's `result`, given the numbers of the issues
    open with the label, the lines the audit denied and the run's address. A cancelled or
    skipped audit says nothing."""
    if result == "failure":
        body = "\n".join([run, "", *(denied or ["The audit gave no report: see the run."])])
        if open_issues:
            return [["comment", str(min(open_issues)), "--body", body]]
        return [["create", "--title", TITLE, "--label", LABEL, "--body", body]]
    if result == "success":
        return [["close", str(number), "--comment", f"Passes again: {run}"]
                for number in sorted(open_issues)]
    return []


def main():
    listed = subprocess.run(["gh", "issue", "list", "--label", LABEL, "--state", "open",
                             "--json", "number"], check=True, capture_output=True, text=True)
    open_issues = [issue["number"] for issue in json.loads(listed.stdout)]
    planned = commands(os.environ["AUDIT"], open_issues, os.environ.get("DENIED", "").splitlines(),
                       os.environ["RUN"])
    if any(command[0] == "create" for command in planned):
        subprocess.run(["gh", "label", "create", LABEL, "--color", "d73a4a", "--force"], check=True)
    for command in planned:
        subprocess.run(["gh", "issue", *command], check=True)


if __name__ == "__main__":
    main()
