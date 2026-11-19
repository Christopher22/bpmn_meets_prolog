"""Opening and saving files, and the protection of unsaved changes."""

from pathlib import Path
from xml.etree import ElementTree

import pytest
from helpers import PROCESS, App
from playwright.sync_api import Dialog, FilePayload, Locator, Page, expect


def payload(name: str, text: str) -> FilePayload:
    return {"name": name, "mimeType": "text/plain", "buffer": text.encode()}


def open_file(app: App, name: str, text: str) -> None:
    app.page.locator("input[type=file]").set_input_files(payload(name, text))


def save(app: App, button: Locator) -> tuple[str, str]:
    """Clicks `button` and returns the name and content of its download."""
    with app.page.expect_download() as download:
        button.click()
    return (
        download.value.suggested_filename,
        Path(download.value.path()).read_text(encoding="utf-8"),
    )


def collect_dialogs(page: Page) -> list[Dialog]:
    """Records dialogs and leaves them open."""
    dialogs: list[Dialog] = []
    page.on("dialog", lambda dialog: dialogs.append(dialog))
    return dialogs


def test_open_replaces_the_program(app: App) -> None:
    open_file(app, "process.pl", PROCESS)
    expect(app.editor).to_have_value(PROCESS)
    expect(app.element("work")).to_be_visible()
    assert app.selection() == (0, 0)


def test_open_normalises_line_endings(app: App) -> None:
    open_file(app, "windows.pl", PROCESS.replace("\n", "\r\n"))
    expect(app.editor).to_have_value(PROCESS)


def test_open_fits_the_new_diagram(app: App) -> None:
    app.zoom_in.click()
    app.zoom_in.click()
    open_file(app, "process.pl", PROCESS)
    expect(app.element("work")).to_be_visible()

    opened = app.viewport()
    app.fit.click()
    assert app.viewport() == pytest.approx(opened)


def test_the_same_file_can_be_opened_again(app: App) -> None:
    open_file(app, "process.pl", PROCESS)
    expect(app.editor).to_have_value(PROCESS)
    # A picker that kept its file would not report picking it again.
    expect(app.page.locator("input[type=file]")).to_have_value("")


def test_save_as_prolog(app: App) -> None:
    app.show_program(PROCESS)
    assert save(app, app.save_prolog) == ("model.pl", PROCESS)


def test_save_as_bpmn(app: App) -> None:
    app.show_program(PROCESS)
    name, xml = save(app, app.save_bpmn)
    assert name == "model.bpmn"

    root = ElementTree.fromstring(xml)
    assert root.tag == "{http://www.omg.org/spec/BPMN/20100524/MODEL}definitions"
    ids = {element.get("id") for element in root.iter()}
    assert {"begin", "work", "finish"} <= ids


def test_saved_files_are_named_after_the_opened_file(app: App) -> None:
    open_file(app, "order.prolog", PROCESS)
    app.wait_rendered()
    expect(app.editor).to_have_value(PROCESS)
    assert save(app, app.save_prolog)[0] == "order.pl"
    assert save(app, app.save_bpmn)[0] == "order.bpmn"


def test_save_as_bpmn_needs_a_rendered_current_program(app: App) -> None:
    app.set_program(PROCESS)
    expect(app.save_bpmn).to_be_disabled()
    app.wait_rendered()

    app.set_program(PROCESS.replace('"Begin"', '"Begin'))
    expect(app.save_bpmn).to_be_disabled()
    expect(app.status).to_contain_text("from the last successful run")
    expect(app.save_bpmn).to_be_disabled()

    app.set_program("fact(a).\n")
    expect(app.status).to_contain_text("No model/1 to render.")
    expect(app.save_bpmn).to_be_disabled()


def test_leaving_without_changes_does_not_ask(app: App) -> None:
    dialogs = collect_dialogs(app.page)
    # Browsers only ask after the user has interacted with the page.
    app.editor.click()
    with app.page.expect_event("close"):
        app.page.close(run_before_unload=True)
    assert dialogs == []


def test_leaving_with_changes_asks(app: App) -> None:
    dialogs = collect_dialogs(app.page)
    app.editor.click()
    app.page.keyboard.type("% changed\n")
    with app.page.expect_event("dialog"):
        app.page.close(run_before_unload=True)
    assert [dialog.type for dialog in dialogs] == ["beforeunload"]
    dialogs[0].dismiss()


def test_saving_clears_the_changes(app: App) -> None:
    dialogs = collect_dialogs(app.page)
    app.editor.click()
    app.page.keyboard.type("% changed\n")
    save(app, app.save_prolog)
    with app.page.expect_event("close"):
        app.page.close(run_before_unload=True)
    assert dialogs == []


def test_opening_a_file_clears_the_changes(app: App) -> None:
    open_file(app, "process.pl", PROCESS)
    expect(app.editor).to_have_value(PROCESS)
    app.editor.click()
    dialogs = collect_dialogs(app.page)
    with app.page.expect_event("close"):
        app.page.close(run_before_unload=True)
    assert dialogs == []


def test_open_keeps_unsaved_changes_if_declined(app: App) -> None:
    app.set_program("changed.\n")
    messages: list[str] = []

    def decline(dialog: Dialog) -> None:
        messages.append(dialog.message)
        dialog.dismiss()

    app.page.on("dialog", decline)
    with app.page.expect_event("dialog"):
        open_file(app, "process.pl", PROCESS)
    assert messages == ["Discard your unsaved changes and open process.pl?"]
    assert app.source() == "changed.\n"


def test_open_replaces_unsaved_changes_if_accepted(app: App) -> None:
    app.set_program("changed.\n")
    app.page.on("dialog", lambda dialog: dialog.accept())
    open_file(app, "process.pl", PROCESS)
    expect(app.editor).to_have_value(PROCESS)
