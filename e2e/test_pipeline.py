"""The edit → check → render pipeline as a whole."""

from helpers import PROCESS, App
from playwright.sync_api import expect


def test_example_renders_on_load(app: App) -> None:
    for pool in ("customer", "vendor"):
        expect(app.element(pool)).to_be_visible()
    expect(app.element("order")).to_contain_text("Order a pizza")
    expect(app.status).to_contain_text("✓ Prolog finished")
    expect(app.status).to_contain_text("✓ bpmn_check/2 found no problems")
    expect(app.page.locator(".output pre")).to_contain_text("10 tasks: [choose,order,")


def test_edit_updates_the_diagram(app: App) -> None:
    app.set_program(PROCESS)
    expect(app.element("work")).to_contain_text("work hard")
    expect(app.element("order")).to_have_count(0)


def test_rapid_typing_ends_with_the_latest_program(app: App) -> None:
    app.show_program(PROCESS)
    app.caret_on('"work hard"', len('"work hard'))
    app.page.keyboard.type(" and then some", delay=30)
    expect(app.element("work")).to_contain_text("and then some")


def test_syntax_error_keeps_the_diagram_until_fixed(app: App) -> None:
    app.show_program(PROCESS)
    app.set_program(PROCESS.replace('"Begin"', '"Begin'))
    expect(app.status).to_contain_text("Diagram and findings are from the last successful run.")
    expect(app.element("work")).to_be_visible()

    app.show_program(PROCESS.replace('"Begin"', '"Started"'))
    expect(app.element("begin_label")).to_contain_text("Started")
    expect(app.status).not_to_contain_text("from the last successful run")
