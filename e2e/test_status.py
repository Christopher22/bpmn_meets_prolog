"""The panel below the editor."""

import re

import pytest
from helpers import ENDLESS, PROBLEMS, PROCESS, App
from playwright.sync_api import Page, expect

PROBLEM_MARKER = re.compile(r"\bbpmn-problem\b")


def test_counts_clauses(app: App) -> None:
    app.set_program("a.\n")
    expect(app.status).to_contain_text("✓ Syntax OK · 1 clause ·")
    app.set_program("a.\nb.\n")
    expect(app.status).to_contain_text("✓ Syntax OK · 2 clauses ·")


def test_warns_about_a_missing_model(app: App) -> None:
    app.set_program("fact(a).\n")
    expect(app.status).to_contain_text("model/1 is not defined, so there is nothing to draw.")
    expect(app.status).to_contain_text("No model/1 to render.")


def test_reports_a_failing_model(app: App) -> None:
    app.set_program(":- use_module(bpmn).\nmodel(M) :- report(M).\n")
    expect(app.status).to_contain_text("model/1: existence_error(procedure,report/1)")


def test_lists_problems_and_outlines_their_elements(app: App) -> None:
    app.show_program(PROBLEMS)
    expect(app.status).to_contain_text("bpmn_check/2 found 2 problem(s):")
    expect(app.status.locator(".findings li")).to_have_text(
        ["finish has no incoming sequence flow", "work has no outgoing sequence flow"]
    )
    expect(app.element("finish")).to_have_class(PROBLEM_MARKER)
    expect(app.element("begin")).not_to_have_class(PROBLEM_MARKER)


def test_clicking_a_syntax_error_shows_it_in_the_editor(app: App) -> None:
    app.set_program('first.\nsecond.\nthird("oops).\n')
    app.select(0)
    app.status.locator("button.jump").click()
    expect(app.editor).to_be_focused()
    assert app.caret_line() == 3


def test_clicking_a_diagnostic_shows_it_in_the_editor(app: App) -> None:
    app.set_program(":- use_module(bpmn).\nfine.\n:- fail.\n")
    expect(app.status).to_contain_text("Not loaded:")
    app.select(0)
    app.status.locator(".findings button.jump").click()
    expect(app.editor).to_be_focused()
    assert app.caret_line() == 3


def test_shows_what_directives_print(app: App) -> None:
    app.set_program(':- use_module(library(format)).\n:- format("hello ~w~n", [world]).\n')
    expect(app.page.locator(".output pre")).to_have_text("hello world")


def test_restarts_prolog_after_the_timeout(app: App) -> None:
    app.set_program(ENDLESS)
    expect(app.status).to_contain_text(
        "⚠ Prolog did not answer within 10 s and was restarted.", timeout=30_000
    )

    app.show_program(PROCESS)
    expect(app.element("work")).to_be_visible()
    expect(app.status).not_to_contain_text("did not answer")


@pytest.mark.allow_page_errors
def test_reports_an_unavailable_worker(no_worker: App) -> None:
    expect(no_worker.status).to_contain_text("Prolog could not start (reload to retry)")
    expect(no_worker.status).to_contain_text("✗ Prolog is unavailable")


@pytest.mark.allow_page_errors
@pytest.mark.xfail(
    strict=True,
    reason="A worker script that fails to load fires a plain Event, not the ErrorEvent "
    "that client.rs expects: reading its message throws, so the page never leaves "
    "'Starting Prolog…'.",
)
def test_reports_a_worker_script_that_cannot_load(page: Page) -> None:
    page.route("**/worker_loader.js", lambda route: route.fulfill(status=404))
    page.goto("/")
    expect(App(page).status).to_contain_text("Prolog could not start", timeout=5_000)
