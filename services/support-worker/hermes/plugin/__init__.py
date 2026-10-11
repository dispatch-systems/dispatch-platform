"""Explicit tool bundles; authorization lives in the host worker and sandbox, never prompts."""
import copy
import json
import os
from pathlib import Path
import subprocess


class Policy:
    def __init__(self, request):
        self.request = request
        self.root = Path(request["worktree"]).resolve(strict=True) if request["worktree"] else None
        self.role = request["role"]
        self.tools = frozenset(request["profile"]["tools"])
        self.activity = []
        self.observations = []
        self.browser = None

    def permit(self, name):
        if name not in self.tools:
            raise PermissionError("Tool is absent from the effective support stage")

    def path(self, raw):
        if self.root is None:
            raise PermissionError("This support stage has no assigned source")
        path = Path(raw)
        if not path.is_absolute():
            path = self.root / path
        path = path.resolve()
        if not path.is_relative_to(self.root):
            raise PermissionError("Path leaves the assigned support worktree")
        if any(part == ".git" or part == ".env" or part.startswith(".env.") or part in ("private", "credentials", "secrets") for part in path.relative_to(self.root).parts):
            raise PermissionError("Private files and Git administration are host owned")
        return path

    def read(self, args, **kwargs):
        self.permit("dispatch_read_file")
        # Copy Hermes's exact pagination/text-file handler into our bundle, with a path guard.
        from tools.file_tools import _handle_read_file
        checked = dict(args)
        checked["path"] = str(self.path(args["path"]))
        if set(checked) - {"path", "offset", "limit"}:
            raise PermissionError("Unknown file options")
        self.activity.append("dispatch_read_file")
        result = _handle_read_file(checked, **kwargs)
        try:
            answer = json.loads(result)
        except (TypeError, ValueError):
            answer = None
        # Hermes returns JSON content for a completed text read, but errors, binary
        # refusals and dedup stubs do not provide fresh source observations.
        if (isinstance(answer, dict) and not answer.get("error")
                and answer.get("success") is not False and not answer.get("is_binary")
                and isinstance(answer.get("content"), str)):
            self.observations.append("dispatch_read_file")
        return result

    def write(self, args, **kwargs):
        self.permit("dispatch_write_file")
        if self.role != "builder":
            raise PermissionError("Only the approved builder can edit")
        path = self.path(args["path"])
        self.activity.append("dispatch_write_file")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(args["content"], encoding="utf-8")
        return json.dumps({"written": str(path.relative_to(self.root))})

    def patch(self, args, **kwargs):
        self.permit("dispatch_patch")
        if self.role != "builder":
            raise PermissionError("Only the approved builder can edit")
        path = self.path(args["path"])
        self.activity.append("dispatch_patch")
        content = path.read_text(encoding="utf-8")
        old = args["old_string"]
        if not old or content.count(old) != 1:
            raise ValueError("Patch must identify exactly one existing string")
        path.write_text(content.replace(old, args["new_string"], 1), encoding="utf-8")
        return json.dumps({"patched": str(path.relative_to(self.root))})

    def command(self, args, **kwargs):
        self.permit("dispatch_command")
        if self.root is None:
            raise PermissionError("This support stage has no assigned source")
        import shutil
        if not shutil.which("bwrap"):
            raise RuntimeError("bubblewrap is required; refusing an unsandboxed command")
        cwd = self.path(args.get("cwd", "."))
        self.activity.append("dispatch_command")
        bind = "--bind" if self.role == "builder" else "--ro-bind"
        command = ["bwrap", "--unshare-all", "--die-with-parent", "--new-session", "--cap-drop", "ALL", "--clearenv", "--ro-bind", "/usr", "/usr", "--ro-bind", "/bin", "/bin", "--ro-bind", "/lib", "/lib", "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--dir", "/sandbox", "--dir", "/sandbox/home", "--dir", "/sandbox/bin", bind, str(self.root), "/workspace", "--setenv", "HOME", "/sandbox/home", "--setenv", "PATH", "/sandbox/bin:/sandbox/.cargo/bin:/usr/local/bin:/usr/bin:/bin", "--setenv", "CARGO_HOME", "/sandbox/.cargo", "--setenv", "RUSTUP_HOME", "/sandbox/.rustup", "--setenv", "CARGO_TARGET_DIR", "/tmp/target", "--setenv", "TMPDIR", "/tmp"]
        # Only compiler binaries and package caches are mounted; never .cargo/credentials or SSH.
        home = Path(os.environ["HOME"])
        for source, destination in [(home / ".cargo/bin", "/sandbox/.cargo/bin"), (home / ".cargo/registry", "/sandbox/.cargo/registry"), (home / ".rustup", "/sandbox/.rustup"), (Path("/lib64"), "/lib64")]:
            if source.exists():
                command += ["--ro-bind", str(source), destination]
        for item in ("/etc/alternatives", "/etc/ld.so.cache"):
            if Path(item).exists():
                command += ["--ro-bind", item, item]
        node = Path(self.request["node"])
        command += ["--ro-bind", str(node.resolve()), "/sandbox/bin/node", "--ro-bind", str(self.root / ".git"), "/workspace/.git", "--chdir", "/workspace/" + str(cwd.relative_to(self.root)), "/bin/bash", "-c", args["command"].replace(str(self.root), "/workspace")]
        result = subprocess.run(command, text=True, capture_output=True, check=False)
        if result.returncode == 0:
            self.observations.append("dispatch_command")
        return json.dumps({"exitCode": result.returncode, "stdout": result.stdout[-100000:], "stderr": result.stderr[-100000:], "sandbox": "assigned-worktree-no-host-network"})

    def diff(self, args, **kwargs):
        self.permit("dispatch_diff")
        if self.root is None:
            raise PermissionError("This support stage has no assigned source")
        self.activity.append("dispatch_diff")
        base = subprocess.run(["git", "rev-parse", "--verify", "origin/main"], cwd=self.root, capture_output=True, check=False)
        result = subprocess.run(["git", "diff", "--no-ext-diff", "--no-textconv", "origin/main...HEAD" if base.returncode == 0 else "HEAD", "--", "."], cwd=self.root, capture_output=True, text=True, check=True)
        pending = subprocess.run(["git", "diff", "--no-ext-diff", "--no-textconv", "HEAD", "--", "."], cwd=self.root, capture_output=True, text=True, check=True)
        self.observations.append("dispatch_diff")
        return json.dumps({"committedDiff":result.stdout[-100000:], "pendingDiff":pending.stdout[-100000:]})

    def browse(self, args, **kwargs):
        self.permit("dispatch_browser")
        self.activity.append("dispatch_browser")
        attempt = Path(self.request["home"]) / "browser-attempt.json"
        attempt.write_text(json.dumps({"attempted": True, "action": args["action"], "sourceSha": self.request["context"]["headSha"]}), encoding="utf-8")
        if self.browser is None:
            bridge = Path(self.request["resources"]) / "browser.mjs"
            self.browser = subprocess.Popen([self.request["node"], str(bridge), str(Path(self.request["home"]) / "request.json")], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.browser.stdin.write(json.dumps(args) + "\n")
        self.browser.stdin.flush()
        line = self.browser.stdout.readline()
        if not line:
            raise RuntimeError("Private verification browser unavailable; inspect runner logs")
        answer = json.loads(line)
        artifact = answer.get("artifact")
        if "error" not in answer and artifact and args["action"] == "screenshot":
            # Reuse Hermes's native vision envelope so the selected Codex model sees pixels;
            # no auxiliary API provider, remote browser or paid image service is called.
            from tools.vision_tools import _build_native_vision_tool_result, _resize_image_for_vision
            image = Path(self.request["home"]) / "evidence" / artifact
            data_url = _resize_image_for_vision(image, mime_type="image/png")
            result = _build_native_vision_tool_result(str(image), "Inspect this actual platform screenshot for the approved criterion and layout.", data_url, image.stat().st_size)
            self.observations.append("dispatch_browser")
            return result
        if "error" not in answer and answer.get("success") is not False:
            self.observations.append("dispatch_browser")
        return line.strip()

    def close(self):
        if self.browser is not None:
            self.browser.stdin.close()
            self.browser.wait()


def register_tools(request, context=None):
    from tools.registry import registry
    from tools.file_tools import READ_FILE_SCHEMA, WRITE_FILE_SCHEMA, PATCH_SCHEMA
    from toolsets import create_custom_toolset, resolve_toolset
    policy = Policy(request)
    profile = request["profile"]
    base = profile["toolset"]
    schemas = {}
    for name, schema in [("dispatch_read_file", READ_FILE_SCHEMA), ("dispatch_write_file", WRITE_FILE_SCHEMA), ("dispatch_patch", PATCH_SCHEMA)]:
        schemas[name] = copy.deepcopy(schema)
        schemas[name]["name"] = name
        schemas[name]["parameters"]["additionalProperties"] = False
    schemas["dispatch_command"] = {"name": "dispatch_command", "description": "Run a command within the assigned source worktree only. Host networking, credentials and filesystem are unavailable. Analyst/reviewer source is read-only. Never weaken required checks.", "parameters": {"type": "object", "properties": {"command": {"type": "string"}, "cwd": {"type": "string"}}, "required": ["command"], "additionalProperties": False}}
    schemas["dispatch_diff"] = {"name": "dispatch_diff", "description": "Read the assigned worktree's current uncommitted diff.", "parameters": {"type": "object", "properties": {}, "additionalProperties": False}}
    schemas["dispatch_browser"] = {"name": "dispatch_browser", "description": "Use an independent real Chromium browser: navigate, snapshot, click, fill, press, reload, viewport, upload, download, screenshot or assert. Upload uses only a generated screenshot fixture and is preview-only. Viewport tests responsive sizes; download reads a delivered artifact without exposing host paths. Supply a current snapshot role/name or CSS selector. Assertions require the exact approved criterion; the host records steps and screenshots. Production mode refuses every request except GET/HEAD and refuses off-origin requests; no JavaScript execution tool exists. Operations against the isolated preview may mutate its fixture data.", "parameters": {"type": "object", "properties": {"action": {"type": "string", "enum": ["navigate", "snapshot", "click", "fill", "press", "reload", "viewport", "upload", "download", "screenshot", "assert"]}, "url": {"type": "string"}, "selector": {"type": "string"}, "role": {"type": "string"}, "name": {"type": "string"}, "value": {"type": "string"}, "criterion": {"type": "string"}, "assertion": {"type": "string", "enum": ["visible", "hidden", "text", "value", "url", "count", "download"]}, "expected": {"type": "string"}, "width": {"type":"integer", "minimum":320, "maximum":1920}, "height": {"type":"integer", "minimum":320, "maximum":1280}}, "required": ["action"], "additionalProperties": False}}
    handlers = {"dispatch_read_file": policy.read, "dispatch_write_file": policy.write, "dispatch_patch": policy.patch, "dispatch_command": policy.command, "dispatch_diff": policy.diff, "dispatch_browser": policy.browse}
    for name in profile["tools"]:
        if context:
            context.register_tool(name, base, schemas[name], handlers[name])
        else:
            registry.register(name=name, toolset=base, schema=schemas[name], handler=handlers[name])
    create_custom_toolset(base, "Dispatch support exact approved tools", tools=profile["tools"], includes=[])
    if set(resolve_toolset(base)) != set(profile["tools"]):
        raise RuntimeError("Resolved tool bundle differs from the versioned manifest")
    return policy


def register(ctx):
    request = json.loads(Path(os.environ["DISPATCH_SUPPORT_REQUEST"]).read_text(encoding="utf-8"))
    return register_tools(request, ctx)
