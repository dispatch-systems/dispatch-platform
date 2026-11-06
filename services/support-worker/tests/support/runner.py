"""Exercise the real runner and tool policy with an in-process model sink."""
import copy
import json
from pathlib import Path
import runpy
import sys
import tempfile
import types
import unittest
from unittest.mock import patch


RESOURCES = Path(__file__).resolve().parents[2] / "hermes"
COOKIE = "fixture-browser-authority"
STORAGE = "fixture-storage-authority"


def module(name, **members):
    value = types.ModuleType(name)
    value.__dict__.update(members)
    return value


class RunnerPrivacy(unittest.TestCase):
    def test_model_input_hides_authority_without_mutating_trusted_request(self):
        metadata = [
            {"origin": "https://fixture.test", "sourceSha": "fixture-sha", "version": None,
             "release": "fixture", "cookie": {"value": COOKIE, "extra": [COOKIE]}},
            {"sourceSha": None, "version": "fixture", "release": "fixture"},
            None, "legacy metadata", [],
        ]
        for images in [False, True]:
            for supplied in [False, True]:
                for deployment in metadata if supplied else [None]:
                    with self.subTest(images=images, supplied=supplied, metadata=deployment):
                        self.run_runner(images, supplied, deployment)

    def run_runner(self, images, supplied, deployment):
        with tempfile.TemporaryDirectory(prefix="dispatch-runner-privacy-") as directory:
            root = Path(directory)
            request_path, result_path = root / "request.json", root / "result.json"
            context = {
                "headSha": "fixture-sha", "rules": "fixture rules", "note": "cookie settings",
                "browser": {"url": "https://fixture.test", "readOnly": True,
                            "storageState": {"cookies": [{"value": STORAGE}], "origins": []}},
            }
            if supplied:
                context["deploymentMetadata"] = copy.deepcopy(deployment)
            if images:
                context["imageInputs"] = [{"path": "fixture.png", "mediaType": "image/png"}]
            request = {
                "home": str(root / "home"), "worktree": None, "role": "reviewer",
                "resources": str(RESOURCES), "instructions": "fixture instructions",
                "profile": {"provider": "fixture", "model": "fixture", "toolset": "fixture",
                            "tools": ["dispatch_browser"], "skills": [],
                            "reasoningEffort": "low", "serviceTier": "default"},
                "context": context, "job": {"id": "fixture-job"},
            }
            request_path.write_text(json.dumps(request), encoding="utf-8")
            handlers, bundles, calls = {}, {}, []

            def register(**tool):
                handlers[tool["name"]] = tool["handler"]

            def toolset(name, description, *, tools, includes):
                bundles[name] = tools

            test = self

            class Agent:
                def __init__(self, **options):
                    self.valid_tool_names = list(handlers)

                def run_conversation(self, content, *, system_message):
                    calls.append(content)
                    encoded = json.dumps(content)
                    test.assertNotIn(COOKIE, encoded, "browser authority leaked to model")
                    test.assertNotIn(STORAGE, encoded, "browser storage leaked to model")
                    payload = json.loads(content[0]["text"] if images else content)
                    expected = copy.deepcopy(context)
                    expected["browser"].pop("storageState")
                    expected.pop("imageInputs", None)
                    if isinstance(expected.get("deploymentMetadata"), dict):
                        expected["deploymentMetadata"].pop("cookie", None)
                    test.assertEqual(payload, {"ticket": request["job"], "context": expected})
                    test.assertIn("fixture rules", system_message)
                    test.assertIn("fixture instructions", system_message)
                    if images:
                        test.assertEqual(content[1], {"type": "image_url", "image_url": {
                            "url": "data:image/png;base64,Zml4dHVyZQ=="}})
                    trusted = handlers["dispatch_browser"].__self__.request
                    test.assertEqual(trusted["context"], context)
                    test.assertEqual(json.loads(request_path.read_text()), request)
                    return {"completed": True, "final_response": "{}"}

            schema = {"parameters": {}}
            modules = {
                "hermes_bootstrap": module("hermes_bootstrap"),
                "hermes_cli": module("hermes_cli"),
                "hermes_cli.runtime_provider": module("hermes_cli.runtime_provider",
                    resolve_runtime_provider=lambda **options: {}),
                "run_agent": module("run_agent", AIAgent=Agent),
                "tools": module("tools"),
                "tools.registry": module("tools.registry", registry=types.SimpleNamespace(register=register)),
                "tools.file_tools": module("tools.file_tools", READ_FILE_SCHEMA=schema,
                    WRITE_FILE_SCHEMA=schema, PATCH_SCHEMA=schema),
                "tools.vision_tools": module("tools.vision_tools",
                    _resize_image_for_vision=lambda *args, **options: "data:image/png;base64,Zml4dHVyZQ=="),
                "toolsets": module("toolsets", create_custom_toolset=toolset,
                    resolve_toolset=lambda name: bundles[name]),
            }
            with patch.dict(sys.modules, modules), patch.object(sys, "argv", [
                str(RESOURCES / "runner.py"), str(root), str(request_path), str(result_path),
            ]):
                try:
                    runpy.run_path(str(RESOURCES / "runner.py"), run_name="__main__")
                except SystemExit:
                    self.fail((root / "home/runner.log").read_text())
            self.assertEqual(len(calls), 1)
            self.assertTrue(json.loads(result_path.read_text())["completed"])


if __name__ == "__main__":
    unittest.main()
