# BPMN in Prolog

Model BPMN as Prolog terms, reason about them with Scryer Prolog, and see them
drawn by bpmn-js. [Everything runs in the browser, try it out!](https://christopher22.github.io/bpmn_meets_prolog/)

The editor on the left holds a Prolog program in the [`bpmn.pl`](prolog/bpmn.pl)
DSL that defines `model/1` (see [`pizza.pl`](prolog/pizza.pl) for the DSL). On
every edit:

1. **Pre-check** (`bpmn-syntax`, main thread, well under a millisecond): one
   linear pass checks quotes, comments, brackets and clause ends and splits
   the program into clauses. The same lexer drives syntax highlighting and
   completion. While you type, or on Ctrl+Space, the editor suggests `bpmn.pl`
   elements with argument templates and the element ids already declared in
   the model. Enter keeps the indentation (one level deeper after `(`, `[` or
   `{`), Tab and Shift+Tab indent and outdent the current or selected lines,
   and Backspace removes indentation a level at a time. Escape, then Tab,
   moves the focus out of the editor. F2, or double-clicking an element in the
   diagram, renames the id, atom or variable at the caret everywhere it is used,
   leaving strings and comments alone.
2. **Prolog** (`bpmn-engine` in a Web Worker): 250 ms after typing pauses, the
   clauses are loaded one by one into Scryer Prolog. `model/1` is linted with
   `bpmn_check/2` and turned into BPMN 2.0 XML with `bpmn_chars/2`.
   Directives run as well, and what they print is shown under the editor.
3. **Diagram** (bpmn-js navigated viewer): the XML is imported, keeping the
   current viewport, and elements with problems are outlined in red.

The buttons in the header open a Prolog program and save the current one as
Prolog (`.pl`) or BPMN 2.0 XML (`.bpmn`). Saving as BPMN is available once the
text in the editor has rendered to a model. With changes since the last save or
open, the browser asks before leaving the page, and Open asks before replacing
them.

## Layout

| Crate           | Role                                                                        |
| --------------- | --------------------------------------------------------------------------- |
| `crates/syntax` | Lenient lexer (`tokenize`), lexical pre-check and clause splitter (`check`). No dependencies. |
| `crates/engine` | Result types, worker protocol and, behind the default `machine` feature, the Scryer Prolog `Engine` with its driver `prolog/driver.pl`. |
| `crates/worker` | The Web Worker binary that owns an `Engine`.                                |
| `crates/web`    | The Leptos page: highlighted editor with completion, status panel, worker client, bpmn-js bindings. |

## Develop

Requirements: Rust (see `rust-toolchain.toml`), `clang` (for the wasm build of
`ring`, a Scryer dependency) and [Trunk](https://trunkrs.dev). The dev
container installs all three.

```sh
cargo test --workspace                       # native tests, incl. real Prolog
cargo clippy --workspace --all-targets
cd crates/web && trunk serve                 # http://127.0.0.1:8080
```

## Deploy

Pull requests are checked by `.github/workflows/ci.yml` (rustfmt, clippy
natively and for wasm32, tests). Every push to `main` builds the site in
release mode and publishes it to GitHub Pages (`.github/workflows/pages.yml`;
set *Settings → Pages → Source* to *GitHub Actions* once). To build by hand:

```sh
cd crates/web && trunk build --release       # static site in crates/web/dist
```

Serve `dist/` from any static host that sends `.wasm` as `application/wasm`.
For a sub-path, add `--public-url /sub/path/`. The build is self-contained and
works without internet access: bpmn-js 18.31.0 is vendored in
`crates/web/vendor/bpmn-js` (with its license, which requires the bpmn.io
watermark to stay visible).

## Scryer Prolog 0.10 notes

These workarounds are documented where they live in the code:

- User programs are never consulted directly. The system loader aborts on the
  first failing directive and leaves the machine unusable, so `driver.pl` reads
  and asserts clause by clause instead. `:- use_module(bpmn).` maps to the
  built-in copy of `bpmn.pl`.
- Every query is wrapped in `catch/3`, because dropping a query that ended in
  an exception panics.
- `Heap::clear` frees a null pointer, which traps on wasm. The worker installs
  a null-tolerant allocator (`crates/worker/src/allocator.rs`).
- Release builds must not use LTO, which turns Scryer's undefined behaviour
  into out-of-bounds accesses on wasm32.
- Debug builds compile Scryer optimised and without debug assertions, because
  a debug-only UB check aborts when a machine is built.

## License
This project is licensed under the [MIT license](LICENSE).