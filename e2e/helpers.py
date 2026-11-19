"""Programs and page interactions shared by the end-to-end tests."""

from playwright.sync_api import Locator, Page, expect

PROCESS = """\
:- use_module(bpmn).

% work is mentioned in this comment.
model(bpmn(m, [
    process(p, "P", [
        start(begin, "Begin"),
        task(work, "work hard"),
        end(finish, "Finish"),
        begin => work => finish
    ])
])).
"""

# `finish` has no incoming flow and `work` no outgoing one.
PROBLEMS = """\
:- use_module(bpmn).
model(bpmn(m, [process(p, "P", [start(begin), task(work, "T"), end(finish), begin => work])])).
"""

# Never answers: only the client's deadline ends it.
ENDLESS = """\
:- use_module(bpmn).
:- repeat, fail.
"""


class App:
    """The page under test, in the terms a user sees it."""

    def __init__(self, page: Page) -> None:
        self.page = page
        self.editor = page.get_by_role("textbox", name="Prolog program")
        self.status = page.locator(".status")
        self.save_prolog = page.get_by_role("button", name="Save as Prolog")
        self.save_bpmn = page.get_by_role("button", name="Save as BPMN")
        self.zoom_in = page.get_by_role("button", name="Zoom in")
        self.zoom_out = page.get_by_role("button", name="Zoom out")
        self.fit = page.get_by_role("button", name="Fit the diagram into the pane")
        self.completions = page.get_by_role("option")
        self.rename_input = page.get_by_role("textbox", name="New name")
        self.rename_message = page.locator(".rename-box [role=status], p.rename-box")

    def source(self) -> str:
        return self.editor.input_value()

    def set_program(self, text: str) -> None:
        """Replaces the program, as pasting over everything would."""
        self.editor.fill(text)

    def show_program(self, text: str) -> None:
        """Replaces the program and waits until its model has rendered."""
        self.set_program(text)
        self.wait_rendered()

    def wait_rendered(self) -> None:
        """Waits until the text in the editor has rendered to a model."""
        expect(self.save_bpmn).to_be_enabled()

    def element(self, element_id: str) -> Locator:
        """The drawn diagram element with the given BPMN id."""
        return self.page.locator(f'.canvas .djs-element[data-element-id="{element_id}"]')

    def select(self, start: int, end: int | None = None) -> None:
        """Focuses the editor and sets its selection, in characters."""
        end = start if end is None else end
        self.editor.evaluate(
            "(area, [start, end]) => { area.focus(); area.setSelectionRange(start, end); }",
            [start, end],
        )

    def caret_on(self, needle: str, offset: int = 0) -> None:
        """Puts the caret `offset` characters into the first `needle`."""
        self.select(self.source().index(needle) + offset)

    def selection(self) -> tuple[int, int]:
        start, end = self.editor.evaluate("area => [area.selectionStart, area.selectionEnd]")
        return start, end

    def selected_text(self) -> str:
        start, end = self.selection()
        return self.source()[start:end]

    def caret_line(self) -> int:
        """The line of the caret, counted from 1."""
        return self.source()[: self.selection()[0]].count("\n") + 1

    def viewport(self) -> tuple[float, float, float]:
        """The diagram's scale and translation."""
        scale, x, y = self.page.locator(".canvas .viewport").first.evaluate(
            "node => { const m = node.transform.baseVal.consolidate().matrix;"
            " return [m.a, m.e, m.f]; }"
        )
        return scale, x, y

    def scale(self) -> float:
        return self.viewport()[0]
