#!/usr/bin/env python3
"""Production's read-only view for the AI Support worker on Dev.

The `dispatch-reader` account's only SSH key runs this as its forced command, so every request
is one read from a fixed list: health, service status, service logs or resources. It reads the
journal, /proc and Production's own health route; never files, databases or configuration.
App logs keep one DSP's events and platform events, never another DSP's or unsigned traffic."""
import argparse
from collections import deque
import datetime
import json
import os
import pwd
import re
import shlex
import subprocess
import sys
import time
import urllib.request

SERVICES = {
    "app": "dispatch-production.service",
    "updater": "dispatch-production-update.service",
    "tunnel": "dispatch-production-tunnel.service",
}
# Status also reports the timer that installs releases: stopping it stops updates.
STATUS_UNITS = [*SERVICES.values(), "dispatch-production-update.timer"]
LOG_OPTIONS = {"service", "since", "until", "level", "route", "request", "dsp"}
MAX_WINDOW = 7 * 86400
MAX_ENTRIES = 300
MAX_LINE = 2000
LEVELS = {"warn": {"warn", "error"}, "error": {"error"}}
RELATIVE = re.compile(r"(\d{1,3})([mhd])")
ABSOLUTE = re.compile(r"\d{4}-\d\d-\d\dT\d\d:\d\d(:\d\d)?Z")
PATTERNS = {
    "dsp": re.compile(r"dsp_[0-9a-f]{32}"),
    "route": re.compile(r"/[A-Za-z0-9/_.{}:-]{0,200}"),
    "request": re.compile(r"req_[0-9a-f]{8,64}"),
}
# Identifiers no investigation needs: the address fingerprint and the hashed account.
DROPPED_FIELDS = ("client", "account")
SECRETS = [
    re.compile(r"(?i)bearer\s+\S+"),
    re.compile(r"(?i)\b(password|secret|token|key)=\S+"),
    re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"),
]


class Refused(Exception):
    pass


def moment(value, now):
    if match := RELATIVE.fullmatch(value):
        return now - int(match[1]) * {"m": 60, "h": 3600, "d": 86400}[match[2]]
    if ABSOLUTE.fullmatch(value):
        text = value.rstrip("Z") + ("" if value.count(":") == 2 else ":00")
        return datetime.datetime.fromisoformat(text).replace(tzinfo=datetime.timezone.utc).timestamp()
    raise Refused("Times are like 30m, 2h, 3d or 2026-10-11T22:00Z")


def parse(words, now):
    """One request: an action and, for logs, options. Anything else is refused."""
    if not words or words[0] not in ("health", "status", "logs", "resources"):
        raise Refused("Actions: health, status, logs, resources")
    action, rest = words[0], words[1:]
    options = {}
    while rest:
        name = rest.pop(0)
        key = name[2:]
        if action != "logs" or not name.startswith("--") or key not in LOG_OPTIONS or key in options:
            raise Refused(f"Unknown or repeated option for {action}")
        if not rest:
            raise Refused(f"{name} needs a value")
        options[key] = rest.pop(0)
    if action != "logs":
        return action, options
    service = options.setdefault("service", "app")
    if service not in SERVICES:
        raise Refused("Services: " + ", ".join(SERVICES))
    since = moment(options.get("since", "1h"), now)
    until = moment(options["until"], now) if "until" in options else now
    if not now - MAX_WINDOW <= since < until:
        raise Refused("Logs reach back at most 7 days, and since must come before until")
    options["since"], options["until"] = int(since), int(until)
    if "level" in options and options["level"] not in LEVELS:
        raise Refused("Levels: warn, error")
    for key, pattern in PATTERNS.items():
        if key in options and not pattern.fullmatch(options[key]):
            raise Refused(f"Invalid {key}")
    if service != "app" and {"dsp", "route", "request"} & options.keys():
        raise Refused("Only app logs have a DSP, route or request")
    return action, options


def journal(arguments):
    """Journal entries as JSON objects, oldest first."""
    process = subprocess.Popen(["journalctl", "--no-pager", "-q", "-o", "json", *arguments],
                               stdout=subprocess.PIPE, text=True, env={"PATH": "/usr/bin:/bin"})
    for line in process.stdout:
        try:
            yield json.loads(line)
        except ValueError:
            continue
    process.wait()


def text(entry):
    message = entry.get("MESSAGE", "")
    # The journal stores a message that isn't valid UTF-8 as a list of bytes.
    if isinstance(message, list):
        message = bytes(message).decode("utf-8", "replace")
    return message if isinstance(message, str) else ""


def stamp(entry):
    seconds = int(entry.get("__REALTIME_TIMESTAMP", "0")) / 1e6
    return datetime.datetime.fromtimestamp(seconds, datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def redact(line):
    for pattern in SECRETS:
        line = pattern.sub("[redacted]", line)
    return line[:MAX_LINE]


def app_event(message, options):
    """The event as it may be shown, or None: one DSP's events and platform events only."""
    try:
        event = json.loads(message)
    except ValueError:
        event = None
    if not isinstance(event, dict):
        # A panic or startup line: platform output, shown unless a route or request is asked for.
        return None if {"route", "request"} & options.keys() else message
    fields = event.get("fields") or {}
    if not isinstance(fields, dict):
        return None
    dsp = fields.get("dspId")
    if dsp is not None:
        if dsp != options.get("dsp"):
            return None
    elif event.get("event") == "http.request" or fields.get("actorId") is not None:
        # Unsigned traffic and other owners' platform requests belong to no ticket.
        return None
    if "level" in options and event.get("level") not in LEVELS[options["level"]]:
        return None
    if "route" in options and not str(fields.get("route", "")).startswith(options["route"]):
        return None
    if "request" in options and fields.get("requestId") != options["request"]:
        return None
    shown = {key: value for key, value in fields.items() if key not in DROPPED_FIELDS}
    return json.dumps({"level": event.get("level"), "event": event.get("event"), "fields": shown},
                      separators=(",", ":"))


def logs(options, uid):
    service = options["service"]
    entries = deque(maxlen=MAX_ENTRIES)
    matched = 0
    for entry in journal([f"--since=@{options['since']}", f"--until=@{options['until']}",
                          f"_UID={uid}", f"_SYSTEMD_USER_UNIT={SERVICES[service]}"]):
        message = text(entry)
        if service == "app":
            message = app_event(message, options)
        elif "level" in options and int(entry.get("PRIORITY", "6")) > (4 if options["level"] == "warn" else 3):
            message = None
        if message is not None:
            matched += 1
            entries.append(f"{stamp(entry)} {redact(message)}")
    return {"service": service, "matched": matched, "shown": len(entries), "entries": list(entries)}


def status(uid, now):
    """Each unit's latest lifecycle message, and its starts and failures over the last day."""
    units = {}
    for unit in STATUS_UNITS:
        latest, starts, failures = None, 0, 0
        for entry in journal([f"--since=@{int(now - MAX_WINDOW)}", f"_UID={uid}", f"USER_UNIT={unit}"]):
            message = text(entry)
            if "Consumed " in message:
                continue
            latest = {"at": stamp(entry), "message": redact(message)}
            if int(entry.get("__REALTIME_TIMESTAMP", "0")) / 1e6 >= now - 86400:
                starts += message.startswith("Started ")
                failures += "Failed" in message or "failed" in message
        units[unit] = {"latest": latest, "startsLastDay": starts, "failuresLastDay": failures}
    return units


def resources():
    with open("/proc/meminfo", encoding="utf-8") as file:
        memory = {key: int(value.split()[0]) // 1024 for key, value in
                  (line.split(":", 1) for line in file) if key in ("MemTotal", "MemAvailable", "SwapTotal", "SwapFree")}
    with open("/proc/loadavg", encoding="utf-8") as file:
        load = [float(value) for value in file.read().split()[:3]]
    with open("/proc/uptime", encoding="utf-8") as file:
        uptime = int(float(file.read().split()[0]))
    disk = os.statvfs("/")
    return {"memoryMB": memory, "load": load, "uptimeSeconds": uptime,
            "diskGB": {"total": round(disk.f_blocks * disk.f_frsize / 1e9, 1),
                       "free": round(disk.f_bavail * disk.f_frsize / 1e9, 1)}}


def health(url):
    with urllib.request.urlopen(url, timeout=10) as response:
        return json.loads(response.read(65536))


def answer(words, settings, now):
    action, options = parse(words, now)
    if action == "health":
        return health(settings.health)
    uid = pwd.getpwnam(settings.owner).pw_uid
    if action == "status":
        return status(uid, now)
    if action == "logs":
        return logs(options, uid)
    return resources()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--owner", required=True, help="the account Production's services run as")
    parser.add_argument("--health", required=True, help="Production's health route on loopback")
    settings = parser.parse_args()
    if not re.fullmatch(r"http://127\.0\.0\.1:\d{2,5}/api/health", settings.health):
        sys.exit("The health route must be on loopback")
    try:
        words = shlex.split(os.environ.get("SSH_ORIGINAL_COMMAND", ""))
        print(json.dumps(answer(words, settings, time.time())))
    except (Refused, ValueError) as error:
        print(json.dumps({"error": str(error)}))
        sys.exit(2)
    except OSError:
        print(json.dumps({"error": "Production did not answer this read"}))
        sys.exit(1)


if __name__ == "__main__":
    main()
