//! Browser front end: edit a `bpmn.pl` program on the left, see its model
//! drawn by bpmn-js on the right.
//!
//! Every edit is first checked by [`bpmn_syntax`] on the main thread (cheap
//! enough for every keystroke). Lexically valid programs are then rendered by
//! [`bpmn_engine`] in a Web Worker (the `bpmn-worker` crate), so Prolog never
//! blocks the page and is not part of the page's bundle.

mod app;
mod bpmn_js;
mod client;
mod completion;
mod components;
mod highlight;
mod indent;
mod syntax;
mod vocabulary;

pub use app::App;
