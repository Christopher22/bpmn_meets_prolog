//! The Prolog worker: owns one [`Engine`] and renders programs on request.
//!
//! Runs as a module Web Worker: Scryer ships an inline JS snippet, which only
//! the `web` target of wasm-bindgen can load. Building the engine happens once
//! at start-up, after which the worker posts [`Response::Ready`].

mod allocator;

use bpmn_engine::Engine;
use bpmn_engine::protocol::{Request, Response};
use js_sys::Date;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

fn main() {
    console_error_panic_hook::set_once();

    let scope: DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let mut engine = Engine::new();

    let reply_to = scope.clone();
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let Some(json) = event.data().as_string() else {
            return;
        };
        let response = match Request::from_json(&json) {
            Ok(request) => render(&mut engine, &request),
            Err(error) => panic!("malformed request from the page: {error}"),
        };
        post(&reply_to, &response);
    });
    scope.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    // The handler lives as long as the worker.
    on_message.forget();

    post(&scope, &Response::Ready);
}

fn render(engine: &mut Engine, request: &Request) -> Response {
    let started = Date::now();
    // The page only sends lexically valid programs; checking again is cheap
    // and yields the clause split the engine needs.
    let result = match bpmn_syntax::check(&request.program) {
        Ok(program) => engine.render(&program).map_err(|error| error.to_string()),
        Err(error) => Err(error.to_string()),
    };
    Response::Rendered {
        id: request.id,
        result,
        millis: Date::now() - started,
    }
}

fn post(scope: &DedicatedWorkerGlobalScope, response: &Response) {
    scope
        .post_message(&response.to_json().into())
        .expect("posting a string to the page cannot fail");
}
