#!/usr/bin/env python3
"""
loom_mcp_wrapper.py

MCP stdio wrapper around `loom proxy` that:
1) Adds curated prompt "recipes" (MCP Prompts) for Zed Agent UX.
2) Polls `tools/list` and emits `notifications/tools/list_changed` when the tool set changes.

This is intentionally dependency-free (stdlib only) and should run anywhere Python 3 is available.
"""

from __future__ import annotations

import argparse
import json
import queue
import subprocess
import sys
import threading
import time
from hashlib import sha256
from typing import Any, Dict, Optional


def _eprint(*args: Any) -> None:
    print(*args, file=sys.stderr, flush=True)


def _write_json(obj: Dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(obj, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def _read_json_line(line: str) -> Dict[str, Any]:
    return json.loads(line)


def _hash_tools_list(result: Dict[str, Any]) -> str:
    tools = result.get("tools") or []
    names = []
    for t in tools:
        if isinstance(t, dict) and isinstance(t.get("name"), str):
            names.append(t["name"])
    names.sort()
    return sha256(("\n".join(names)).encode("utf-8")).hexdigest()


PROMPT_PREFIX = "loom_zed__"

PROMPT_RECIPES = [
    {
        "name": f"{PROMPT_PREFIX}onboard_repo",
        "description": "Onboard to this repo quickly (structure, workflows, risks).",
        "arguments": [],
        "template": (
            "You are my coding copilot. Onboard to this repository.\n\n"
            "1) Summarize what this repo does and where the important entrypoints are.\n"
            "2) Identify the build/lint/test commands.\n"
            "3) Call Loom tools to discover relevant services, configs, or deploy targets.\n"
            "4) Produce a short map: directories, key files, and how changes flow to prod.\n"
        ),
    },
    {
        "name": f"{PROMPT_PREFIX}triage_ci",
        "description": "Triage a failing CI job and propose a minimal fix.",
        "arguments": [],
        "template": (
            "Help me triage CI failures.\n\n"
            "1) Determine what failed and why.\n"
            "2) Propose the smallest safe change.\n"
            "3) If relevant, call Loom tools for CI logs, git history, or related incidents.\n"
            "4) Provide a step-by-step verification plan.\n"
        ),
    },
    {
        "name": f"{PROMPT_PREFIX}k8s_rollout_check",
        "description": "Kubernetes rollout checklist (safe steps + verification).",
        "arguments": [],
        "template": (
            "Give me a safe Kubernetes rollout checklist for this change.\n\n"
            "Include: what to check before, how to deploy, how to verify, and rollback steps.\n"
            "Use Loom tools to inspect cluster state if available.\n"
        ),
    },
    {
        "name": f"{PROMPT_PREFIX}security_quickscan",
        "description": "Quick security scan (secrets, deps, risky patterns) and mitigations.",
        "arguments": [],
        "template": (
            "Do a quick security scan of the change/repo.\n\n"
            "Check for secrets, unsafe subprocess usage, injection risks, and dependency issues.\n"
            "Use Loom tools where useful, and suggest mitigations with minimal disruption.\n"
        ),
    },
]


def _prompt_list() -> Dict[str, Any]:
    return {
        "prompts": [
            {
                "name": p["name"],
                "description": p["description"],
                "arguments": p.get("arguments", []),
            }
            for p in PROMPT_RECIPES
        ]
    }


def _prompt_get(name: str, arguments: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
    _ = arguments or {}
    for p in PROMPT_RECIPES:
        if p["name"] == name:
            # MCP prompt result returns "messages" the client can add to the conversation.
            return {
                "description": p["description"],
                "messages": [
                    {
                        "role": "user",
                        "content": {"type": "text", "text": p["template"]},
                    }
                ],
            }
    raise KeyError(name)


class Child:
    def __init__(self, cmd: list[str]) -> None:
        self.proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        assert self.proc.stdin and self.proc.stdout
        self._write_lock = threading.Lock()

    def send(self, obj: Dict[str, Any]) -> None:
        line = json.dumps(obj, separators=(",", ":")) + "\n"
        with self._write_lock:
            self.proc.stdin.write(line)
            self.proc.stdin.flush()

    def close(self) -> None:
        try:
            self.proc.terminate()
        except Exception:
            pass


def main() -> int:
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("--loom", default="loom", help="Path to the loom binary.")
    ap.add_argument(
        "--tools-poll-interval-secs",
        type=int,
        default=30,
        help="Poll tools/list every N seconds; 0 disables polling.",
    )
    ap.add_argument(
        "--disable-prompt-recipes",
        action="store_true",
        default=False,
        help="Disable Loom Zed prompt recipes.",
    )
    ap.add_argument("child_args", nargs=argparse.REMAINDER)
    ns = ap.parse_args()

    enable_prompts = not ns.disable_prompt_recipes

    child_cmd = [ns.loom]
    if ns.child_args and ns.child_args[0] == "--":
        child_cmd.extend(ns.child_args[1:])
    else:
        child_cmd.extend(ns.child_args)

    child = Child(child_cmd)

    # Map of request ids that should be intercepted/rewritten on the way back.
    intercept_prompts_list_ids: set[Any] = set()
    intercept_tools_list_ids: set[Any] = set()
    initialize_id: Any = None

    # Tool set change detection state.
    tools_hash_lock = threading.Lock()
    last_tools_hash: Optional[str] = None
    poll_inflight = False

    outbound_q: "queue.Queue[Dict[str, Any]]" = queue.Queue()

    def forward_to_client(obj: Dict[str, Any]) -> None:
        outbound_q.put(obj)

    def reader_thread() -> None:
        nonlocal last_tools_hash, poll_inflight
        assert child.proc.stdout
        for line in child.proc.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                msg = _read_json_line(line)
            except Exception:
                # If the child emits non-JSON output on stdout, ignore it.
                _eprint("non-json child stdout:", line)
                continue

            msg_id = msg.get("id")
            if msg_id is not None and msg_id == initialize_id and isinstance(msg.get("result"), dict):
                # We emit `notifications/tools/list_changed`, so declare `tools.listChanged`.
                # This nudges MCP clients (like Zed) to refresh tools when notified.
                caps = msg["result"].get("capabilities")
                if isinstance(caps, dict):
                    tools_caps = caps.get("tools")
                    if not isinstance(tools_caps, dict):
                        tools_caps = {}
                        caps["tools"] = tools_caps
                    tools_caps["listChanged"] = True

            if msg_id in intercept_prompts_list_ids:
                intercept_prompts_list_ids.discard(msg_id)
                # Merge child prompts with our recipes (ours first).
                result = msg.get("result") if isinstance(msg.get("result"), dict) else {}
                merged = _prompt_list()
                child_prompts = []
                if isinstance(result, dict):
                    child_prompts = result.get("prompts") or []
                if isinstance(child_prompts, list):
                    merged["prompts"].extend(child_prompts)
                msg["result"] = merged
                forward_to_client(msg)
                continue

            if msg_id in intercept_tools_list_ids:
                intercept_tools_list_ids.discard(msg_id)
                with tools_hash_lock:
                    poll_inflight = False
                    if isinstance(msg.get("result"), dict):
                        new_hash = _hash_tools_list(msg["result"])
                        if last_tools_hash is None:
                            last_tools_hash = new_hash
                        elif new_hash != last_tools_hash:
                            last_tools_hash = new_hash
                            forward_to_client(
                                {
                                    "jsonrpc": "2.0",
                                    "method": "notifications/tools/list_changed",
                                }
                            )
                # Do not forward poll responses to the client.
                continue

            # Opportunistically update baseline on any tools/list response.
            if isinstance(msg.get("result"), dict) and msg_id is not None:
                # Heuristic: tools/list responses contain a "tools" array.
                if "tools" in msg["result"]:
                    with tools_hash_lock:
                        last_tools_hash = _hash_tools_list(msg["result"])

            forward_to_client(msg)

    def poller_thread() -> None:
        nonlocal poll_inflight
        interval = int(ns.tools_poll_interval_secs or 0)
        if interval <= 0:
            return
        counter = 0
        while True:
            time.sleep(interval)
            with tools_hash_lock:
                if poll_inflight:
                    continue
                poll_inflight = True
            counter += 1
            poll_id = f"__loom_zed_tools_poll_{counter}"
            intercept_tools_list_ids.add(poll_id)
            child.send({"jsonrpc": "2.0", "id": poll_id, "method": "tools/list", "params": {}})

    t_reader = threading.Thread(target=reader_thread, name="loom-child-reader", daemon=True)
    t_reader.start()

    t_poller = threading.Thread(target=poller_thread, name="loom-tools-poller", daemon=True)
    t_poller.start()

    # Writer loop in a dedicated thread so we can keep reading stdin promptly.
    def writer_thread() -> None:
        while True:
            msg = outbound_q.get()
            _write_json(msg)

    t_writer = threading.Thread(target=writer_thread, name="loom-client-writer", daemon=True)
    t_writer.start()

    # Main loop: read from client (Zed) and forward/intercept.
    try:
        for line in sys.stdin:
            line = line.strip()
            if not line:
                continue
            try:
                msg = _read_json_line(line)
            except Exception:
                # Ignore garbage input.
                continue

            method = msg.get("method")
            if enable_prompts and method == "prompts/list":
                # Merge child prompts with our recipes.
                intercept_prompts_list_ids.add(msg.get("id"))
                child.send(msg)
                continue

            if method == "initialize":
                initialize_id = msg.get("id")

            if enable_prompts and method == "prompts/get":
                params = msg.get("params") if isinstance(msg.get("params"), dict) else {}
                name = params.get("name")
                arguments = params.get("arguments") if isinstance(params.get("arguments"), dict) else {}
                if isinstance(name, str) and name.startswith(PROMPT_PREFIX):
                    try:
                        result = _prompt_get(name, arguments)
                        forward_to_client({"jsonrpc": "2.0", "id": msg.get("id"), "result": result})
                    except KeyError:
                        forward_to_client(
                            {
                                "jsonrpc": "2.0",
                                "id": msg.get("id"),
                                "error": {"code": -32602, "message": f"unknown prompt: {name}"},
                            }
                        )
                    continue

            # Default: proxy through to loom.
            child.send(msg)

    except KeyboardInterrupt:
        pass
    finally:
        child.close()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
