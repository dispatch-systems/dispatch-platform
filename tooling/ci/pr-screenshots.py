#!/usr/bin/env python3
"""Before-and-after screenshots for a PR: capture them from the fixture server, then upload
them to the screenshots bucket and print the PR's Screenshots section.

    npm run pr:screenshots -- capture <before|after> <screen>... [--dark]
    npm run pr:screenshots -- publish [--reviewed]

Screens are page ids from `dashboard/src/app/route-meta.ts`. Captures go to the worktree's
scratch directory, `/tmp/dispatch-<worktree>/screenshots/<label>/`. Publishing audits them
with the export privacy check first; the images it has not seen need a visual review, which
`--reviewed` asserts, and are then recorded in the private review manifest. Each image then
goes to R2 under its SHA-256, through the private upload settings, and the section links it
through the screenshots Worker.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import hmac
import html
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request

LABELS = ("before", "after")
NEEDS_REVIEW = "image-needs-synthetic-data-review"
# The audit also flags every unreviewed binary asset as such; the same review clears both.
REVIEW_RULES = {NEEDS_REVIEW, "asset-needs-synthetic-data-review"}
SETTINGS = Path.home() / ".config/dispatch-screenshots/upload.json"
SETTING_FIELDS = ("endpoint", "bucket", "accessKeyId", "secretAccessKey", "publicUrl")
# A key names its bytes, so a link never changes what it shows and may be cached for good.
CACHE_CONTROL = "public, max-age=31536000, immutable"


def run(args, cwd=None, env=None, check=True, capture=True):
    result = subprocess.run(args, cwd=cwd, env=env, text=True, capture_output=capture)
    if check and result.returncode:
        raise SystemExit(f"{' '.join(map(str, args[:3]))} failed:\n{(result.stderr or result.stdout or '').strip()}")
    return (result.stdout or "").strip()


class Worktree:
    def __init__(self, root=None):
        self.root = Path(root or run(["git", "rev-parse", "--show-toplevel"])).resolve()
        self.name = self.root.name
        self.scratch = Path(os.environ.get("DISPATCH_SCRATCH") or f"/tmp/dispatch-{self.name}")
        self.shots = self.scratch / "screenshots"
        self.branch = run(["git", "rev-parse", "--abbrev-ref", "HEAD"], cwd=self.root)
        # The workspace holds the worktrees directory and the private `.privacy/` state.
        self.workspace = Path(os.environ.get("DISPATCH_WORKSPACE") or self.root.parent.parent)


def capture(tree, label, screens, dark):
    if not (tree.root / ".build/release.json").is_file():
        raise SystemExit("Run npm run build first; the screenshots come from the built dashboard.")
    output = tree.shots / label
    if output.exists():
        shutil.rmtree(output)
    env = {**os.environ, "DISPATCH_SCREENSHOTS": ",".join(screens), "DISPATCH_SCREENSHOT_DIR": str(output),
           "DISPATCH_SCREENSHOT_SCHEME": "dark" if dark else "light", "TMPDIR": str(tree.scratch),
           "DISPATCH_TEST_OUTPUT": str(tree.scratch / "test-results")}
    tree.scratch.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(["npm", "run", "test:ui", "--", "tests/browser/screenshots.spec.ts"],
                            cwd=tree.root, env=env)
    if result.returncode:
        raise SystemExit("The capture failed; its output is above.")
    for name in sorted(output.glob("*.png")):
        print(name)


def files(tree):
    """`(label, name, path)` for every capture, in the order the section shows them."""
    found = []
    for label in LABELS:
        directory = tree.shots / label
        if directory.is_dir():
            for path in sorted(directory.glob("*.png")):
                found.append((label, path.name, path))
    if not found:
        raise SystemExit(f"No screenshots under {tree.shots}; capture some first.")
    return found


def titles(tree):
    result = {}
    for label in LABELS:
        index = tree.shots / label / "index.json"
        if index.is_file():
            result.update(json.loads(index.read_text()))
    return result


def audit(tree, staged, review_file, tesseract):
    """The export privacy check over the staged copies; its findings, name to rules."""
    args = [sys.executable, str(tree.root / "tooling/security/exports.py"), "--workspace", str(staged),
            "--review-file", str(review_file)]
    if tesseract:
        args += ["--tesseract", str(tesseract)]
    result = subprocess.run(args, cwd=tree.root, text=True, capture_output=True)
    findings = {}
    for line in (result.stdout + result.stderr).splitlines():
        match = re.fullmatch(r"(\S+):(\d+): (\S+)", line.strip())
        if match:
            findings.setdefault(match.group(1), set()).add(match.group(3))
    if result.returncode and not findings:
        raise SystemExit(f"The export audit could not run:\n{(result.stderr or result.stdout).strip()}")
    return findings


def record(review_file, hashes):
    """Adds the reviewed images' exact hashes to the private manifest, keeping it private."""
    manifest = json.loads(review_file.read_text())
    manifest.setdefault("assets", {}).update(hashes)
    review_file.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    review_file.chmod(0o600)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def upload_settings(path=None):
    """The private upload settings: R2's S3 endpoint, the bucket, a key that can write only that
    bucket, and the screenshots Worker's public URL. The file holds the key's secret."""
    path = Path(path or os.environ.get("DISPATCH_SCREENSHOTS_SETTINGS") or SETTINGS)
    if not path.is_file():
        raise SystemExit(
            f"No upload settings at {path}. Write it, mode 0600, as a JSON object: endpoint (R2's S3 endpoint, "
            "https://<account id>.r2.cloudflarestorage.com), bucket, accessKeyId and secretAccessKey (an R2 "
            "API token with Object Read & Write on that bucket alone), and publicUrl (the screenshots Worker's "
            "workers.dev URL).")
    if path.stat().st_mode & 0o077:
        raise SystemExit(f"{path} holds a secret; make it mode 0600.")
    settings = json.loads(path.read_text())
    missing = [field for field in SETTING_FIELDS if not settings.get(field)]
    if missing:
        raise SystemExit(f"{path} lacks {', '.join(missing)}.")
    return settings


def sign(method, url, headers, payload_hash, key_id, secret, region="auto"):
    """The Authorization value for an S3 request, by AWS Signature Version 4, which R2 takes with
    the region `auto`. Signs the host and every header given; `x-amz-date` must be one."""
    parts = urllib.parse.urlsplit(url)
    signed = {"host": parts.netloc, **{name.lower(): value.strip() for name, value in headers.items()}}
    names = ";".join(sorted(signed))
    request = "\n".join([method, parts.path or "/", parts.query,
                         "".join(f"{name}:{signed[name]}\n" for name in sorted(signed)), names, payload_hash])
    stamp = signed["x-amz-date"]
    scope = f"{stamp[:8]}/{region}/s3/aws4_request"
    text = "\n".join(["AWS4-HMAC-SHA256", stamp, scope, hashlib.sha256(request.encode()).hexdigest()])
    key = f"AWS4{secret}".encode()
    for part in scope.split("/"):
        key = hmac.new(key, part.encode(), hashlib.sha256).digest()
    signature = hmac.new(key, text.encode(), hashlib.sha256).hexdigest()
    return f"AWS4-HMAC-SHA256 Credential={key_id}/{scope}, SignedHeaders={names}, Signature={signature}"


def upload(settings, captured, opener=urllib.request.urlopen):
    """Puts each distinct image in the bucket as `<sha256>.png`; `(label, name)` to its public URL."""
    urls, sent = {}, set()
    for label, name, path in captured:
        body = path.read_bytes()
        digest = hashlib.sha256(body).hexdigest()
        urls[(label, name)] = f"{settings['publicUrl'].rstrip('/')}/{digest}.png"
        if digest in sent:
            continue
        url = f"{settings['endpoint'].rstrip('/')}/{settings['bucket']}/{digest}.png"
        headers = {"Cache-Control": CACHE_CONTROL, "Content-Type": "image/png", "x-amz-content-sha256": digest,
                   "x-amz-date": datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")}
        headers["Authorization"] = sign("PUT", url, headers, digest, settings["accessKeyId"],
                                        settings["secretAccessKey"])
        try:
            with opener(urllib.request.Request(url, data=body, headers=headers, method="PUT"), timeout=60):
                pass
        except urllib.error.HTTPError as error:
            # R2 answers with an XML error; its code alone says what went wrong.
            code = re.search(r"<Code>([A-Za-z]+)</Code>", error.read().decode(errors="replace"))
            raise SystemExit(f"Uploading {label}/{name} failed: HTTP {error.code}"
                             + (f" {code.group(1)}" if code else ""))
        except urllib.error.URLError as error:
            raise SystemExit(f"Uploading {label}/{name} failed: {error.reason}")
        sent.add(digest)
    return urls


def section(captured, urls, labels):
    """The PR's Screenshots section: a collapsed dropdown per screen, holding its before and after,
    or one of them alone for a new or a removed screen."""
    screens = {}
    for label, name, _ in captured:
        screens.setdefault(name, {})[label] = urls[(label, name)]
    lines = ["## Screenshots", ""]
    for name, shots in screens.items():
        title = labels.get(name[:-4], name[:-4].replace("-", " ").capitalize())
        pair = len(shots) == 2
        kind = "before and after" if pair else "new" if "after" in shots else "removed"
        # GitHub renders Markdown inside the dropdown only between blank lines.
        lines += ["<details>", f"<summary>{html.escape(title)}, {kind}</summary>", ""]
        for label in LABELS:
            if label in shots:
                lines += [label.capitalize(), ""] if pair else []
                lines += [f"![{title} {label}]({shots[label]})", ""]
        lines += ["</details>", ""]
    return "\n".join(lines).rstrip("\n") + "\n"


def publish(tree, reviewed, review_file=None, tesseract=None, settings=None, uploader=upload, auditor=audit):
    if tree.branch in ("main", "HEAD"):
        raise SystemExit("Publish from the PR's branch, not main.")
    captured = files(tree)
    settings = settings or upload_settings()
    review_file = review_file or tree.workspace / ".privacy/export-review.json"
    if tesseract is None:
        candidate = tree.workspace / ".privacy/bin/tesseract"
        tesseract = candidate if candidate.is_file() else None
    with tempfile.TemporaryDirectory(prefix="dispatch-screenshots-") as temporary:
        staged = Path(temporary) / "workspace"
        keys, audited = {}, []
        for label, name, path in captured:
            copy = staged / "screenshots" / tree.branch / label / name
            copy.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, copy)
            keys[f"screenshots/{tree.branch}/{label}/{name}"] = path
            audited.append((label, name, copy))
        findings = auditor(tree, staged, review_file, tesseract)
        unreviewed = {name for name, rules in findings.items() if rules <= REVIEW_RULES}
        other = {name: rules for name, rules in findings.items() if not rules <= REVIEW_RULES}
        if other:
            listed = "\n".join(f"- {name}: {', '.join(sorted(rules))}" for name, rules in sorted(other.items()))
            raise SystemExit(f"The export audit found problems; matching values are withheld:\n{listed}")
        if unreviewed:
            if not reviewed:
                listed = "\n".join(f"- {keys[name]}" for name in sorted(unreviewed))
                raise SystemExit("Look at these images, then publish again with --reviewed to record them "
                                 f"as reviewed synthetic screenshots:\n{listed}")
            record(review_file, {name: sha256(keys[name]) for name in unreviewed})
            if auditor(tree, staged, review_file, tesseract):
                raise SystemExit("The export audit still fails after recording the review.")
        # Upload the audited copies, so a capture changed since cannot go out unchecked.
        text = section(captured, uploader(settings, audited), titles(tree))
    (tree.shots / "section.md").write_text(text)
    print(text)
    return text


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    cap = commands.add_parser("capture", help="photograph screens into the scratch directory")
    cap.add_argument("label", choices=LABELS)
    cap.add_argument("screens", nargs="+", help="page ids from app/route-meta.ts")
    cap.add_argument("--dark", action="store_true", help="capture the dark color scheme")
    pub = commands.add_parser("publish", help="audit, upload to the screenshots bucket, print the section")
    pub.add_argument("--reviewed", action="store_true",
                     help="the listed images were looked at and show only fixture data")
    args = parser.parse_args(argv)
    tree = Worktree()
    if args.command == "capture":
        capture(tree, args.label, args.screens, args.dark)
    else:
        publish(tree, args.reviewed)


if __name__ == "__main__":
    main()
