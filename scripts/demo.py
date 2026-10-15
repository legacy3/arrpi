# /// script
# requires-python = ">=3.12"
# dependencies = ["rich==15.0.0"]
# ///
"""Record an actual Claude conversation with the local fake ArrPi stack."""

import argparse
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import threading
import time
from urllib.request import Request, urlopen

from rich.console import Console
from rich.markdown import Markdown

ROOT = Path(__file__).resolve().parents[1]
ORIGINS = ("http://127.0.0.1:4174", "http://127.0.0.1:4173")
TOOLS = (
    "list_movies",
    "list_series",
    "search_library",
    "upcoming_releases",
    "search_media",
    "media_details",
    "media_details_batch",
    "request_movie",
    "request_series",
    "request_status",
    "my_requests",
    "report_issue",
    "close_issue",
)
PROMPTS = (
    (
        "Library recommendations",
        "Recommend a comedy I can watch tonight: under two hours and rated at least 7.",
    ),
    (
        "Upcoming releases",
        "What is on our movie and TV calendar for the next 7 days (2026-10-15 to 2026-10-22)?",
    ),
    (
        "Movie requests",
        "Find Moonberry Express. If it is missing, request it with my default profile and check the request status.",
    ),
    (
        "Selected TV seasons",
        "For Moonberry Station, request only season 2. Leave season 1 alone.",
    ),
    (
        "Owned issues",
        "The Last Lantern has quiet dialogue. Report an audio issue, then close that issue as a test.",
    ),
    (
        "Batch details and links",
        "Batch-check The Last Lantern, Moonberry Express and Moonberry Station. Show availability and plain watch/request URLs.",
    ),
)
SYSTEM = (
    "You are a concise personal media assistant. Use ArrPi MCP for every media "
    "fact and action. Answer in English, under 45 words per turn. "
    "No emoji, tables, preambles or commentary about your tools. These are "
    "fictional fixture records. Never invent facts or pretend an action succeeded. "
    "When asked to request a movie, request it through ArrPi with Seerr's default "
    "profile, then verify it with request_status. Do not poll media_details after "
    "requesting: a new request has not downloaded yet. Approval is not availability. "
    "For selected TV seasons, request exactly the seasons the user named. "
    "Use search_library for recommendations with structured filters. "
    "For batch details and links, use media_details_batch rather than repeating "
    "individual calls. It already includes links. Compact bullet lists are fine."
)
THEME = (
    "18141b,ece5e9,18141b,e9959f,b3c99a,e7c28e,b6b0df,d9a1c0,abc9bf,ece5e9,"
    "847b88,e9959f,b3c99a,e7c28e,b6b0df,d9a1c0,abc9bf,ffffff"
)


def executable(name):
    resolved = shutil.which(name)
    if resolved is None:
        raise SystemExit(f"Install {name} before recording the demo.")
    return resolved


def fixture_account(origin, name):
    fixture = json.loads((ROOT / "tests/arrpi.json").read_text())
    reader = next(client for client in fixture["clients"] if client["name"] == "reader")
    account = next(client for client in fixture["clients"] if client["name"] == name)
    if not reader["key"].startswith("fake-") or not account["key"].startswith("fake-"):
        raise SystemExit("The demo requires fake fixture credentials.")
    request = Request(
        origin + "/v1/config", headers={"Authorization": "Bearer " + reader["key"]}
    )
    with urlopen(request, timeout=15) as response:
        settings = json.load(response)["settings"]
    if settings.get("seerr", {}).get("url") != "http://fixtures:5055/":
        raise SystemExit("Refusing to run outside the local fake Seerr fixture stack.")
    if not settings["mcp"]["enabled"]:
        raise SystemExit("The fixture MCP server must be enabled.")
    return account


def command(origin, account, session):
    config = session / "mcp.json"
    config.write_text(
        json.dumps(
            {
                "mcpServers": {
                    "arrpi": {
                        "type": "http",
                        "url": origin + "/v1/mcp",
                        "headers": {"Authorization": "Bearer " + account["key"]},
                    }
                }
            }
        )
    )
    return [
        executable("claude"),
        "-p",
        "--model",
        "fable",
        "--effort",
        "low",
        "--restricted",
        "--tools",
        "",
        "--allowedTools",
        *["mcp__arrpi__" + tool for tool in TOOLS],
        "--strict-mcp-config",
        "--mcp-config",
        str(config),
        "--disable-slash-commands",
        "--no-session-persistence",
        "--permission-mode",
        "dontAsk",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--system-prompt",
        SYSTEM,
    ]


def turn(process, prompt, transcript, console):
    console.print("You", style="bold #e7b69b")
    for character in prompt:
        console.print(character, end="", markup=False, highlight=False)
        time.sleep(0.018)
    console.print("\n")
    process.stdin.write(
        json.dumps({"type": "user", "message": {"role": "user", "content": prompt}})
        + "\n"
    )
    process.stdin.flush()
    calls = []
    pending = set()
    for raw in process.stdout:
        transcript.write(raw)
        transcript.flush()
        event = json.loads(raw)
        if event["type"] == "assistant":
            for block in event["message"]["content"]:
                if block["type"] == "tool_use":
                    name = block["name"]
                    if not name.startswith("mcp__arrpi__"):
                        raise RuntimeError("Unexpected non-ArrPi tool in the demo.")
                    calls.append(name)
                    pending.add(block["id"])
                    console.print(
                        "  arrpi." + name.removeprefix("mcp__arrpi__"), style="dim"
                    )
                elif block["type"] == "text" and block["text"].strip():
                    console.print("\nClaude", style="bold #d9a1c0")
                    console.print(Markdown(block["text"]))
        if event["type"] == "user":
            for block in event["message"]["content"]:
                if isinstance(block, dict) and block.get("type") == "tool_result":
                    if block.get("is_error"):
                        raise RuntimeError("An actual MCP tool returned an error.")
                    pending.discard(block["tool_use_id"])
        if event["type"] == "result":
            if event.get("is_error") or not calls or pending:
                raise RuntimeError(
                    "The demo turn did not complete with actual MCP calls."
                )
            console.print()
            time.sleep(6)
            return calls
    raise RuntimeError("Claude exited before completing the conversation.")


def conversation(origin, session, name):
    account = fixture_account(origin, name)
    console = Console(
        force_terminal=True, color_system="truecolor", no_color=False, width=92
    )
    invocation = command(origin, account, session)
    with (session / "claude.stderr").open("w") as errors, (
        session / "transcript.jsonl"
    ).open("w") as transcript:
        process = subprocess.Popen(
            invocation,
            cwd=session,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=errors,
            text=True,
            encoding="utf-8",
        )
        timeout = threading.Timer(300, process.kill)
        timeout.daemon = True
        timeout.start()
        try:
            calls = []
            for index, (title, prompt) in enumerate(PROMPTS, start=1):
                console.clear()
                console.print("ArrPi  /  Claude Code", style="bold #e7b69b")
                console.print(
                    f"{index}/{len(PROMPTS)}  {title}  /  Local demo library\n",
                    style="dim",
                )
                calls.extend(turn(process, prompt, transcript, console))
            required = {
                "mcp__arrpi__" + name
                for name in (
                    "search_library",
                    "upcoming_releases",
                    "request_movie",
                    "request_status",
                    "request_series",
                    "report_issue",
                    "close_issue",
                    "media_details_batch",
                )
            }
            if not required.issubset(calls):
                raise RuntimeError(
                    "The demo must complete every workflow with actual MCP calls."
                )
            (session / "evidence.json").write_text(
                json.dumps(
                    {"origin": origin, "prompts": PROMPTS, "tool_calls": calls},
                    indent=2,
                )
                + "\n"
            )
        finally:
            timeout.cancel()
            process.stdin.close()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--origin", choices=ORIGINS, default=ORIGINS[0])
    parser.add_argument(
        "--account", choices=("requester-1", "requester-2"), default="requester-2"
    )
    parser.add_argument(
        "--output", type=Path, default=ROOT / "docs/images/mcp-demo.gif"
    )
    parser.add_argument("--evidence-dir", type=Path)
    parser.add_argument("--conversation", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    os.umask(0o077)
    session = (
        args.evidence_dir or Path(tempfile.mkdtemp(prefix="arrpi-demo-"))
    ).resolve()
    if session.is_relative_to(ROOT):
        raise SystemExit(
            "Keep raw transcripts and MCP configuration outside the repository."
        )
    session.mkdir(parents=True, exist_ok=True, mode=0o700)
    if args.conversation:
        conversation(args.origin, session, args.account)
        return
    recording = session / "conversation.cast"
    inner = [
        executable("uv"),
        "run",
        "--locked",
        str(Path(__file__).resolve()),
        "--conversation",
        "--origin",
        args.origin,
        "--account",
        args.account,
        "--evidence-dir",
        str(session),
    ]
    subprocess.run(
        [
            executable("asciinema"),
            "rec",
            "--headless",
            "--return",
            "--overwrite",
            "--window-size",
            "92x30",
            "--output-format",
            "asciicast-v2",
            "--command",
            shlex.join(inner),
            str(recording),
        ],
        env={**os.environ, "TERM": "xterm-256color"},
        check=True,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            executable("agg"),
            "--theme",
            THEME,
            "--font-family",
            "DejaVu Sans Mono",
            "--font-size",
            "16",
            "--line-height",
            "1.25",
            "--fps-cap",
            "60",
            "--idle-time-limit",
            "5",
            "--last-frame-duration",
            "8",
            "--renderer",
            "resvg",
            str(recording),
            str(args.output),
        ],
        check=True,
    )
    print(f"Saved {args.output}\nRaw conversation and tool evidence: {session}")


if __name__ == "__main__":
    main()
