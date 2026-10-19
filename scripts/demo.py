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
from urllib.parse import urlencode
from urllib.request import Request, urlopen

from rich.console import Console
from rich.markdown import Markdown

ROOT = Path(__file__).resolve().parents[1]
ORIGINS = ("http://127.0.0.1:4174", "http://127.0.0.1:4173")
TOOLS = (
    "search_library",
    "library_summary",
    "queue_summary",
    "service_status",
    "my_quota",
    "upcoming_releases",
    "search_media",
    "media_details",
    "media_details_batch",
    "request_profiles",
    "request_media_batch",
    "request_movie",
    "request_series",
    "request_status",
    "my_requests",
    "report_issue",
    "close_issues_batch",
    "refresh_media",
    "media_command_status",
    "series_episodes",
    "search_releases",
)
PROMPTS = (
    (
        "Find something worth watching",
        "Find two available comedy movies under two hours, IMDb 7+ with 10,000+ votes. Compare their IMDb and Rotten Tomatoes ratings.",
    ),
    (
        "Plan the week",
        "What is coming October 15–22, 2026? Which releases can I watch already?",
    ),
    (
        "Request a movie batch",
        "Request Moonberry Express and The Paper Planet together with my 1080p profile. Check their status.",
    ),
    (
        "Find missing episodes",
        "Refresh Moonberry Station, then search for its missing aired episodes. Check that both commands finish.",
    ),
)

SYSTEM = (
    "You are a concise personal media assistant. Use ArrPi MCP for every media "
    "fact and action. Answer in English, under 40 words per turn. "
    "No emoji, tables, preambles or commentary about your tools. These are "
    "fictional fixture records. Never invent facts or pretend an action succeeded. "
    "When asked to request media, use the explicitly chosen profile or Seerr's default "
    "profile, then verify it with request_status. Do not poll media_details after "
    "requesting: a new request has not downloaded yet. Approval is not availability. "
    "For selected TV seasons, request exactly the seasons the user named. "
    "Use search_library for recommendations with structured filters. "
    "For maintenance, wait for media_command_status to finish before reading episodes; "
    "use native source and item/episode IDs from tool results, never guess them. "
    "Use batch requests and batch issue closing when the user names multiple items. "
    "For rating comparisons, batch details and links, use media_details_batch rather than repeating "
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


def command(origin, account, session, model):
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
        model,
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
            time.sleep(7)
            return calls
    raise RuntimeError("Claude exited before completing the conversation.")


def conversation(origin, session, name, model):
    account = fixture_account(origin, name)
    console = Console(
        force_terminal=True, color_system="truecolor", no_color=False, width=92
    )
    invocation = command(origin, account, session, model)
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
        timeout = threading.Timer(600, process.kill)
        timeout.daemon = True
        timeout.start()
        try:
            calls = []
            console.clear()
            console.print("ArrPi  /  Claude Code", style="bold #e7b69b")
            console.print("Local demo library\n", style="dim")
            for index, (title, prompt) in enumerate(PROMPTS, start=1):
                label = f"{index}/{len(PROMPTS)}  {title}"
                console.rule(label if index == 1 else "Next: " + label, style="#847b88")
                time.sleep(1)
                calls.extend(turn(process, prompt, transcript, console))
            required = {
                "mcp__arrpi__" + name
                for name in (
                    "search_library",
                    "upcoming_releases",
                    "request_media_batch",
                    "request_profiles",
                    "refresh_media",
                    "media_command_status",
                    "series_episodes",
                    "search_releases",
                    "request_status",
                    "media_details_batch",
                )
            }
            if not required.issubset(calls):
                raise RuntimeError(
                    "The demo must complete every workflow with actual MCP calls."
                )
            console.rule("Demo complete", style="#e7b69b")
            time.sleep(3)
            (session / "evidence.json").write_text(
                json.dumps(
                    {
                        "origin": origin,
                        "model": model,
                        "prompts": PROMPTS,
                        "tool_calls": calls,
                    },
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


def upload(path, session):
    """Use the same attachment endpoint as GitHub CLI, without posting an issue."""
    repo = subprocess.check_output(
        [executable("gh"), "api", "repos/{owner}/{repo}", "--jq", ".id"],
        cwd=ROOT,
        text=True,
    ).strip()
    token = subprocess.check_output(
        [executable("gh"), "auth", "token"], text=True
    ).strip()
    endpoint = "https://uploads.github.com/user-attachments/assets?" + urlencode(
        {"name": path.name, "content_type": "video/mp4", "repository_id": repo}
    )
    with path.open("rb") as content:
        request = Request(
            endpoint,
            data=content,
            headers={
                "Authorization": "Bearer " + token,
                "Content-Type": "application/octet-stream",
                "Content-Length": str(path.stat().st_size),
                "Accept": "application/vnd.github+json",
            },
            method="POST",
        )
        with urlopen(request, timeout=120) as response:
            url = json.load(response)["url"]
    if not url.startswith("https://github.com/user-attachments/assets/"):
        raise RuntimeError("GitHub returned an unexpected attachment URL")
    (session / "video-url.txt").write_text(url + "\n")
    print("GitHub video: " + url)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--origin", choices=ORIGINS, default=ORIGINS[0])
    parser.add_argument(
        "--account", choices=("requester-1", "requester-2"), default="requester-2"
    )
    parser.add_argument(
        "--output", type=Path, default=ROOT / "docs/images/mcp-demo.mp4"
    )
    parser.add_argument("--evidence-dir", type=Path)
    parser.add_argument(
        "--upload",
        action="store_true",
        help="Upload the video as a GitHub README attachment",
    )
    parser.add_argument(
        "--model",
        default="sonnet",
        help="Claude model for the recording (default: sonnet)",
    )
    parser.add_argument("--conversation", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.output.suffix not in {".mp4", ".gif"}:
        parser.error("Choose an .mp4 or .gif output")
    if args.upload and args.output.suffix != ".mp4":
        parser.error("GitHub video uploads require an .mp4 output")
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
        conversation(args.origin, session, args.account, args.model)
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
        "--model",
        args.model,
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
    gif = args.output if args.output.suffix == ".gif" else session / "demo.gif"
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
            "7",
            "--speed",
            "1.2",
            "--last-frame-duration",
            "3",
            "--renderer",
            "resvg",
            str(recording),
            str(gif),
        ],
        check=True,
    )
    if args.output.suffix != ".gif":
        subprocess.run(
            [
                executable("ffmpeg"),
                "-y",
                "-loglevel",
                "error",
                "-i",
                str(gif),
                "-vf",
                "fps=30",
                "-c:v",
                "libx264",
                "-crf",
                "18",
                "-preset",
                "slow",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                "-an",
                str(args.output),
            ],
            check=True,
        )
    if args.upload:
        upload(args.output, session)
    print(f"Saved {args.output}\nRaw conversation and tool evidence: {session}")


if __name__ == "__main__":
    main()
