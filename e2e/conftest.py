"""Builds and serves the site, and opens it for each test."""

import functools
import http.server
import os
import subprocess
import threading
from collections.abc import Iterator
from pathlib import Path

import pytest
from helpers import App
from playwright.sync_api import Page, expect

WEB = Path(__file__).parent.parent / "crates" / "web"

# The first render starts Scryer Prolog in a worker.
expect.set_options(timeout=20_000)


class SiteHandler(http.server.SimpleHTTPRequestHandler):
    """Serves the site with the media type browsers require for wasm."""

    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".js": "text/javascript",
    }

    def log_message(self, format: str, *args: object) -> None:
        pass


@pytest.fixture(scope="session")
def base_url() -> Iterator[str]:
    """Builds the release site, the profile that ships, and serves it.

    Set E2E_SKIP_BUILD=1 to rerun against the existing `crates/web/dist`.
    """
    if not os.environ.get("E2E_SKIP_BUILD"):
        subprocess.run(["trunk", "build", "--release"], cwd=WEB, check=True)
    handler = functools.partial(SiteHandler, directory=str(WEB / "dist"))
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    yield f"http://127.0.0.1:{server.server_address[1]}"
    server.shutdown()


@pytest.fixture(autouse=True)
def browser_errors(page: Page, request: pytest.FixtureRequest) -> Iterator[list[str]]:
    """Fails any test during which the page reports an error, such as a panic."""
    errors: list[str] = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.on(
        "console",
        lambda message: errors.append(message.text) if message.type == "error" else None,
    )
    yield errors
    if not request.node.get_closest_marker("allow_page_errors"):
        assert errors == []


@pytest.fixture
def app(page: Page) -> App:
    """The page with the example program rendered."""
    page.goto("/")
    opened = App(page)
    opened.wait_rendered()
    expect(opened.element("order")).to_be_visible()
    return opened


@pytest.fixture
def no_worker(page: Page) -> App:
    """The page with its Prolog worker unable to start."""
    page.route("**/worker_bg.wasm", lambda route: route.fulfill(status=404))
    page.goto("/")
    return App(page)
