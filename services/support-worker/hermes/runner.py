"""Private host runner using Hermes's supported AIAgent core without the updating CLI wrapper."""
import contextlib
import importlib.util
import json
import os
from pathlib import Path
import sys

# Verified artifact resources are immutable, including their inventory. Isolated mode
# ignores PYTHONDONTWRITEBYTECODE, so prevent cache creation explicitly before bootstrap.
sys.dont_write_bytecode = True

source, request_path, result_path = map(Path, sys.argv[1:4])
sys.path.insert(0, str(source))
request = json.loads(request_path.read_text(encoding="utf-8"))
home = Path(request["home"])
home.mkdir(parents=True, exist_ok=True)
os.environ["HERMES_HOME"] = str(home)
cwd = request["worktree"] or str(home)
os.environ["TERMINAL_CWD"] = cwd
os.environ["TERMINAL_ENV"] = "local"
os.environ["DISPATCH_SUPPORT_REQUEST"] = str(request_path)
# Owned profile configuration prevents personal tool discovery and provider fallback. JSON
# is valid YAML, so this uses Hermes's supported config loader without a second parser.
(home / "config.yaml").write_text(json.dumps({"model":{"provider":request["profile"]["provider"],"default":request["profile"]["model"]},"plugins":{"enabled":[]},"tools":{"tool_search":{"enabled":"off"}},"fallback_model":None}), encoding="utf-8")
(home / "config.yaml").chmod(0o600)
# Hermes package-manager bootstrap selects the installed dependency closure; no CLI update.
import hermes_bootstrap

with (home / "runner.log").open("w", encoding="utf-8") as log, contextlib.redirect_stdout(log), contextlib.redirect_stderr(log):
    try:
        plugin_path = Path(request["resources"]) / "plugin/__init__.py"
        spec = importlib.util.spec_from_file_location("dispatch_support_plugin", plugin_path)
        plugin = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(plugin)
        policy = plugin.register_tools(request)
        from hermes_cli.runtime_provider import resolve_runtime_provider
        from run_agent import AIAgent
        profile = request["profile"]
        runtime = resolve_runtime_provider(requested=profile["provider"], target_model=profile["model"])
        skills = "\n\n".join((Path(request["resources"]) / "skills" / name / "SKILL.md").read_text(encoding="utf-8") for name in profile["skills"])
        rules = request["context"].get("rules", "")
        instructions = "\n\n".join([skills, rules, request["instructions"]])
        agent = AIAgent(provider=profile["provider"], requested_provider=profile["provider"], model=profile["model"], api_key=runtime.get("api_key"), base_url=runtime.get("base_url"), api_mode=runtime.get("api_mode"), enabled_toolsets=[profile["toolset"]], reasoning_config={"effort": profile["reasoningEffort"]}, service_tier=profile["serviceTier"], skip_memory=True, skip_background_review=True, skip_context_files=True, load_soul_identity=False, quiet_mode=True, cwd=cwd)
        # Dynamic MCP/plugin additions fail startup rather than silently broadening capabilities.
        if set(agent.valid_tool_names) != set(profile["tools"]):
            raise PermissionError("Effective Hermes tools differ from the profile's exact manifest: " + json.dumps({"expected":profile["tools"], "actual":sorted(agent.valid_tool_names)}))
        safe_context = dict(request["context"])
        safe_context["browser"] = {key: value for key, value in request["context"].get("browser", {}).items() if key != "storageState"}
        images = safe_context.pop("imageInputs", [])
        payload = {"ticket": request["job"], "context": safe_context}
        content = [{"type":"text", "text":json.dumps(payload)}]
        for image in images:
            try:
                from tools.vision_tools import _resize_image_for_vision
                image_path = Path(image["path"])
                mime = image["mediaType"]
                if mime == "image/gif":
                    from PIL import Image
                    with Image.open(image_path) as gif:
                        converted = home / (image_path.name + ".png")
                        gif.seek(0)
                        gif.convert("RGBA").save(converted, "PNG")
                    image_path, mime = converted, "image/png"
                    content.append({"type":"text","text":"This GIF attachment is inspected as its first static frame; animation is unverified."})
                data_url = _resize_image_for_vision(image_path, mime_type=mime, max_dimension=1568)
                content.append({"type":"image_url", "image_url":{"url":data_url}})
            except Exception:
                content.append({"type":"text","text":"One reporter screenshot could not be decoded. Explicitly preserve this uncertainty and continue the browser/source investigation."})
        result = agent.run_conversation(content if images else json.dumps(payload), system_message=instructions)
        (home / "transcript.json").write_text(json.dumps(result, default=str), encoding="utf-8")
        if not result.get("completed") or result.get("error"):
            raise RuntimeError("Hermes did not finish the authorized run")
        final = result["final_response"].strip()
        if final.startswith("```"):
            final = final.split("\n", 1)[1].rsplit("```", 1)[0]
        value = json.loads(final)
        result_path.write_text(json.dumps({"completed": True, "result": value, "toolActivity":policy.activity, "toolObservations":policy.observations}), encoding="utf-8")
        policy.close()
    except Exception:
        import traceback
        traceback.print_exc()
        sys.exit(1)
