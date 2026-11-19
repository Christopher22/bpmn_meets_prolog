"""Moving around the diagram."""

import pytest
from helpers import PROCESS, App
from playwright.sync_api import expect

MIN_SCALE, MAX_SCALE = 0.2, 4


@pytest.mark.allow_page_errors
def test_zoom_is_disabled_before_the_first_diagram(no_worker: App) -> None:
    expect(no_worker.page.locator(".overlay")).to_contain_text("Waiting for the first model")
    for button in (no_worker.zoom_in, no_worker.zoom_out, no_worker.fit):
        expect(button).to_be_disabled()


def test_buttons_zoom_in_and_out(app: App) -> None:
    fitted = app.scale()
    app.zoom_in.click()
    zoomed_in = app.scale()
    assert zoomed_in > fitted

    app.zoom_out.click()
    app.zoom_out.click()
    assert app.scale() < zoomed_in


def test_zoom_stops_at_its_limits(app: App) -> None:
    for _ in range(25):
        app.zoom_in.click()
    assert app.scale() == pytest.approx(MAX_SCALE)
    for _ in range(40):
        app.zoom_out.click()
    assert app.scale() == pytest.approx(MIN_SCALE)


def test_fit_returns_to_the_whole_diagram(app: App) -> None:
    fitted = app.viewport()
    app.zoom_in.click()
    app.zoom_in.click()
    app.fit.click()
    assert app.viewport() == pytest.approx(fitted)


def test_buttons_work_from_the_keyboard(app: App) -> None:
    fitted = app.scale()
    app.zoom_in.focus()
    app.page.keyboard.press("Enter")
    assert app.scale() > fitted


def test_viewport_survives_edits(app: App) -> None:
    app.show_program(PROCESS)
    app.zoom_in.click()
    before = app.viewport()

    app.show_program(PROCESS.replace('"Begin"', '"Started"'))
    expect(app.element("begin_label")).to_contain_text("Started")
    # bpmn-js rounds the scale when restoring a viewbox.
    assert app.viewport() == pytest.approx(before, abs=0.001)


def test_the_wheel_zooms(app: App) -> None:
    fitted = app.scale()
    box = app.page.locator(".canvas").bounding_box()
    assert box is not None
    app.page.mouse.move(box["x"] + box["width"] / 2, box["y"] + box["height"] / 2)
    app.page.keyboard.down("Control")
    app.page.mouse.wheel(0, -200)
    app.page.keyboard.up("Control")
    app.page.wait_for_function(
        "fitted => document.querySelector('.canvas .viewport')"
        ".transform.baseVal.consolidate().matrix.a > fitted",
        arg=fitted,
    )


def test_dragging_pans(app: App) -> None:
    scale, x, y = app.viewport()
    box = app.page.locator(".canvas").bounding_box()
    assert box is not None
    # Below the example diagram, on the empty canvas.
    start = (box["x"] + box["width"] / 2, box["y"] + box["height"] - 40)
    app.page.mouse.move(*start)
    app.page.mouse.down()
    app.page.mouse.move(start[0] + 60, start[1] - 30, steps=5)
    app.page.mouse.up()

    assert app.viewport() == pytest.approx((scale, x + 60, y - 30), abs=1)


@pytest.mark.parametrize("size", [(1400, 800), (420, 700)])
def test_the_watermark_stays_visible(app: App, size: tuple[int, int]) -> None:
    app.page.set_viewport_size({"width": size[0], "height": size[1]})
    watermark = app.page.locator(".bjs-powered-by")
    watermark.scroll_into_view_if_needed()
    expect(watermark).to_be_visible()

    mark = watermark.bounding_box()
    pane = app.page.locator(".diagram-pane").bounding_box()
    controls = app.page.locator(".zoom-controls").bounding_box()
    assert mark is not None and pane is not None and controls is not None
    assert pane["x"] <= mark["x"] and mark["x"] + mark["width"] <= pane["x"] + pane["width"]
    assert pane["y"] <= mark["y"] and mark["y"] + mark["height"] <= pane["y"] + pane["height"]
    assert controls["y"] + controls["height"] <= mark["y"]
