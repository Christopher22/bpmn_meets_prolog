"""The completion list."""

from helpers import PROCESS, App
from playwright.sync_api import expect

LABEL = ".completion-label"


def type_new(app: App, text: str, program: str = "") -> None:
    """Types `text` at the end of `program`."""
    app.set_program(program)
    app.select(len(program))
    app.page.keyboard.type(text)


def test_opens_while_typing_with_templates(app: App) -> None:
    type_new(app, "ta")
    expect(app.completions.locator(LABEL)).to_have_text(
        ['task(Id, "Name")', 'task(Id, "Name", user)']
    )
    expect(app.editor).to_have_attribute("aria-expanded", "true")


def test_offers_declared_ids_first(app: App) -> None:
    type_new(app, "wo", PROCESS + "\n")
    expect(app.completions.locator(LABEL).first).to_have_text("work")


def test_ctrl_space_offers_everything(app: App) -> None:
    app.set_program("")
    app.select(0)
    expect(app.completions).to_have_count(0)
    app.page.keyboard.press("Control+Space")
    expect(app.completions.locator(LABEL).first).to_have_text("and(Id)")
    expect(app.completions.locator(LABEL).filter(has_text="bpmn(Id, [])")).to_have_count(1)


def test_nothing_is_offered_inside_comments_and_strings(app: App) -> None:
    type_new(app, "% ta")
    expect(app.completions).to_have_count(0)
    type_new(app, '"ta')
    expect(app.completions).to_have_count(0)


def test_enter_inserts_the_template_and_selects_its_first_argument(app: App) -> None:
    type_new(app, "ta")
    app.page.keyboard.press("Enter")
    assert app.source() == 'task(Id, "Name")'
    assert app.selected_text() == "Id"
    expect(app.completions).to_have_count(0)


def test_arrow_keys_move_the_selection_and_tab_accepts(app: App) -> None:
    type_new(app, "ta")
    first, second = app.completions.nth(0), app.completions.nth(1)
    expect(first).to_have_attribute("aria-selected", "true")

    app.page.keyboard.press("ArrowDown")
    expect(second).to_have_attribute("aria-selected", "true")
    app.page.keyboard.press("ArrowDown")
    expect(first).to_have_attribute("aria-selected", "true")
    app.page.keyboard.press("ArrowUp")
    expect(second).to_have_attribute("aria-selected", "true")

    app.page.keyboard.press("Tab")
    assert app.source() == 'task(Id, "Name", user)'


def test_click_accepts(app: App) -> None:
    type_new(app, "ta")
    app.completions.nth(1).click()
    assert app.source() == 'task(Id, "Name", user)'
    expect(app.editor).to_be_focused()


def test_escape_closes_without_changes(app: App) -> None:
    type_new(app, "ta")
    expect(app.completions).not_to_have_count(0)
    app.page.keyboard.press("Escape")
    expect(app.completions).to_have_count(0)
    assert app.source() == "ta"
    expect(app.editor).to_have_attribute("aria-expanded", "false")


def test_keeps_an_existing_argument_list(app: App) -> None:
    app.set_program('ta(x, "T").')
    app.select(2)
    app.page.keyboard.press("Control+Space")
    app.page.keyboard.press("Enter")
    assert app.source() == 'task(x, "T").'
