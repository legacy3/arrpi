# /// script
# requires-python = ">=3.12"
# dependencies = ["playwright==1.64.0"]
# ///
"""Refresh README screenshots using the local Docker fixtures and demo data."""

import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys
from urllib.parse import quote, urlsplit
from urllib.request import Request, urlopen

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
ORIGIN = "http://127.0.0.1:4173"


def read_json(path, key):
    request = Request(ORIGIN + path, headers={"Authorization": "Bearer " + key})
    with urlopen(request, timeout=20) as response:
        return json.load(response)


def demo_responses(key):
    status = read_json("/v1/status", key)
    settings = read_json("/v1/config", key)
    account = next(c for c in settings["settings"]["clients"] if c.get("seerr_user"))
    mcp = read_json("/v1/mcp/status?client=" + quote(account["name"], safe=""), key)
    for tool in mcp["tools"]:
        tool["available"] = True
        tool["reason"] = None
    account["name"] = "alex"
    settings["settings"]["clients"] = [account]
    settings["settings"]["mcp"]["public_url"] = "https://arrpi.example.com/v1/mcp"
    for index, source in enumerate(status["sources"]):
        source["counters"] = {
            "hits": 825 if index else 1247,
            "misses": 41 if index else 63,
            "stale": 0,
            "upstream_failures": 0,
        }
    summaries = {}
    for kind in ("library", "queue", "status"):
        document = read_json("/v1/summaries/" + kind, key)
        for source in document["sources"]:
            if source["status"] == "ok":
                source["freshness"]["cache"] = "hit"
                source["freshness"]["age_seconds"] = 12
        summaries["/v1/summaries/" + kind] = document
    events = [
        {
            "date": "2026-10-15T18:00:00Z",
            "episode": 3,
            "has_file": True,
            "id": 1,
            "kind": "episode",
            "season": 2,
            "series": "Moonberry Station",
            "source": "sonarr",
            "title": "The missing teapot",
        },
        {
            "date": "2026-10-15T20:00:00Z",
            "episode": 5,
            "has_file": False,
            "id": 2,
            "kind": "episode",
            "season": 3,
            "series": "Cloudberry Kitchen",
            "source": "sonarr",
            "title": "A pinch of stardust",
        },
        {
            "date": "2026-10-16",
            "has_file": True,
            "id": 3,
            "kind": "digital",
            "source": "radarr",
            "title": "The Last Lantern",
        },
        {
            "date": "2026-10-16T19:00:00Z",
            "episode": 4,
            "has_file": False,
            "id": 4,
            "kind": "episode",
            "season": 2,
            "series": "Starlight Post Office",
            "source": "sonarr",
            "title": "Letters from the moon",
        },
        {
            "date": "2026-10-17",
            "has_file": False,
            "id": 5,
            "kind": "cinema",
            "source": "radarr",
            "title": "A Very Small Planet",
        },
        {
            "date": "2026-10-18",
            "has_file": True,
            "id": 6,
            "kind": "physical",
            "source": "radarr",
            "title": "The Tangerine Express",
        },
    ]
    return {
        **summaries,
        "/v1/status": status,
        "/v1/config": settings,
        "/v1/calendar": {
            "events": events,
            "sources": [
                {"age_seconds": 12, "cache": "hit", "source": s["name"], "status": "ok"}
                for s in status["sources"]
                if s["calendar"]
            ],
        },
        "/v1/mcp/status": mcp,
        "/v1/config/mcp/stats": {
            "items": [
                {
                    "calls": 184,
                    "last_used": "2026-10-15T11:45:00Z",
                    "outcome": "ok",
                    "tool": "search_media",
                },
                {
                    "calls": 42,
                    "last_used": "2026-10-15T11:50:00Z",
                    "outcome": "ok",
                    "tool": "media_details",
                },
            ]
        },
        "/v1/config/mcp/audit": {
            "items": [
                {
                    "id": 1,
                    "actor_id": 2,
                    "created_at": "2026-10-15T11:45:00Z",
                    "tool": "request_movie",
                    "target": {
                        "tmdb_id": 123456,
                        "request_id": 7,
                        "media_type": "movie",
                        "title": "The Last Lantern",
                    },
                    "outcome": "approved",
                }
            ],
            "limit": 20,
            "offset": 0,
            "total": 1,
        },
    }


def capture(output, key, replies):
    output.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        if not Path(playwright.chromium.executable_path).exists():
            subprocess.run(
                [sys.executable, "-m", "playwright", "install", "chromium"], check=True
            )
        browser = playwright.chromium.launch()
        try:
            page = browser.new_page(
                viewport={"width": 1360, "height": 980}, device_scale_factor=1
            )
            page.clock.set_fixed_time(datetime(2026, 10, 15, 12, tzinfo=timezone.utc))
            page.route("**/v1/**", lambda route: respond(route, replies))
            page.goto(ORIGIN)
            page.get_by_label("Client key", exact=True).fill(key)
            page.get_by_role("button", name="Connect", exact=True).click()
            for name, height in (("Overview", 720), ("Calendar", 900), ("MCP", 800)):
                page.set_viewport_size({"width": 1360, "height": height})
                if name != "Overview":
                    page.get_by_role("link", name=name, exact=True).click()
                page.get_by_role("heading", name=name, exact=True).wait_for()
                if name == "Overview":
                    page.get_by_text("Queued jobs", exact=True).wait_for()
                if name == "Calendar":
                    page.get_by_text("The Last Lantern", exact=True).wait_for()
                if name == "MCP":
                    page.get_by_text(
                        "Seerr connected. Requests available.", exact=True
                    ).wait_for()
                page.wait_for_load_state("networkidle")
                page.evaluate("document.fonts.ready")
                path = output / (name.lower() + ".png")
                page.screenshot(
                    path=path, animations="disabled", full_page=name == "Overview"
                )
                print(path.relative_to(ROOT) if path.is_relative_to(ROOT) else path)
        finally:
            browser.close()


def respond(route, replies):
    path = urlsplit(route.request.url).path
    if route.request.method != "GET":
        route.abort()
    elif path in replies:
        route.fulfill(json=replies[path])
    else:
        route.continue_()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--no-build", action="store_true", help="reuse the running fixture stack"
    )
    parser.add_argument(
        "--output", type=Path, default=ROOT / "docs/images", help="screenshot directory"
    )
    args = parser.parse_args()
    if not args.no_build:
        subprocess.run(
            [
                "docker",
                "compose",
                "-p",
                "arrpi-test",
                "-f",
                "deploy/compose.test.yaml",
                "up",
                "--build",
                "-d",
                "--wait",
            ],
            cwd=ROOT,
            check=True,
        )
    config = json.loads((ROOT / "tests/arrpi.json").read_text())
    key = next(c["key"] for c in config["clients"] if c["name"] == "reader")
    capture(args.output.resolve(), key, demo_responses(key))


if __name__ == "__main__":
    main()
