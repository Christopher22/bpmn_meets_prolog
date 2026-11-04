//! Drives the Prolog worker from the page.
//!
//! At most one request is in flight. Submitting while the worker is busy
//! replaces the pending program, so a burst of edits costs one extra render
//! at most. A worker that does not answer in time (say, user code looping
//! forever) or that crashes is terminated and replaced by a fresh one. A
//! worker that crashes before it is ready is not replaced: it would only
//! crash again.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use bpmn_engine::Rendering;
use bpmn_engine::protocol::{Request, Response};
use gloo_timers::callback::Timeout;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen_futures::spawn_local;
use web_sys::{ErrorEvent, MessageEvent, Worker, WorkerOptions, WorkerType};

/// What the client reports to the page.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A worker is loading and building its engine.
    Starting,
    /// The worker is idle.
    Ready,
    /// The worker is rendering a program.
    Busy,
    /// A render finished.
    Rendered {
        /// The program that was rendered.
        program: String,
        /// The rendering, or why the engine could not produce one.
        result: Result<Rendering, String>,
        /// Time spent in Prolog, in milliseconds.
        millis: f64,
    },
    /// The worker did not answer in time and was replaced.
    TimedOut,
    /// The worker failed and was replaced.
    Crashed(String),
    /// The worker failed to start; there is no Prolog until a reload.
    Unavailable(String),
}

/// A handle to the Prolog worker. Cloning shares the worker.
#[derive(Clone)]
pub struct Client {
    inner: Rc<RefCell<Inner>>,
}

struct Inner {
    script: String,
    timeout_ms: u32,
    on_event: Rc<dyn Fn(Event)>,
    connection: Option<Connection>,
    ready: bool,
    next_id: u64,
    in_flight: Option<InFlight>,
    pending: Option<String>,
}

/// A live worker and the handlers attached to it.
struct Connection {
    worker: Worker,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.worker.set_onmessage(None);
        self.worker.set_onerror(None);
        self.worker.terminate();
    }
}

struct InFlight {
    id: u64,
    program: String,
    _deadline: Timeout,
}

impl Client {
    /// Spawns the worker at `script` (a classic worker script URL).
    ///
    /// `on_event` is called for every state change, never re-entrantly.
    pub fn new(script: &str, timeout_ms: u32, on_event: impl Fn(Event) + 'static) -> Self {
        let client = Self {
            inner: Rc::new(RefCell::new(Inner {
                script: script.to_owned(),
                timeout_ms,
                on_event: Rc::new(on_event),
                connection: None,
                ready: false,
                next_id: 0,
                in_flight: None,
                pending: None,
            })),
        };
        restart(&client.inner, Event::Starting);
        client
    }

    /// Renders `program` as soon as the worker is free.
    pub fn submit(&self, program: String) {
        self.inner.borrow_mut().pending = Some(program);
        let events = pump(&self.inner);
        emit(&self.inner, events);
    }
}

/// Replaces the worker (if any) with a fresh one and reports `reason`.
fn restart(this: &Rc<RefCell<Inner>>, reason: Event) {
    let mut events = vec![reason];
    let connection = connect(this);
    {
        let mut inner = this.borrow_mut();
        // Dropping the old connection terminates its worker.
        inner.connection = None;
        inner.in_flight = None;
        inner.ready = false;
        match connection {
            Ok(connection) => {
                inner.connection = Some(connection);
                if !matches!(events[0], Event::Starting) {
                    events.push(Event::Starting);
                }
            }
            Err(message) => events.push(Event::Unavailable(message)),
        }
    }
    emit(this, events);
}

/// Terminates a worker that failed before becoming ready.
fn give_up(this: &Rc<RefCell<Inner>>, message: String) {
    {
        let mut inner = this.borrow_mut();
        inner.connection = None;
        inner.in_flight = None;
    }
    emit(this, vec![Event::Unavailable(message)]);
}

fn connect(this: &Rc<RefCell<Inner>>) -> Result<Connection, String> {
    let script = this.borrow().script.clone();
    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    let worker = Worker::new_with_options(&script, &options)
        .map_err(|error| format!("cannot start worker: {error:?}"))?;

    let weak = Rc::downgrade(this);
    let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        if let Some(this) = weak.upgrade() {
            on_message(&this, &event);
        }
    });

    let weak = Rc::downgrade(this);
    let on_error = Closure::<dyn FnMut(ErrorEvent)>::new(move |event: ErrorEvent| {
        event.prevent_default();
        let message = event.message();
        // Restarting drops this very closure, so leave the handler first.
        defer(&weak, move |this| {
            if this.borrow().ready {
                restart(this, Event::Crashed(message));
            } else {
                give_up(this, message);
            }
        });
    });

    worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    Ok(Connection {
        worker,
        _on_message: on_message,
        _on_error: on_error,
    })
}

fn on_message(this: &Rc<RefCell<Inner>>, event: &MessageEvent) {
    let response = event
        .data()
        .as_string()
        .ok_or_else(|| "non-text message".to_owned())
        .and_then(|json| Response::from_json(&json).map_err(|error| error.to_string()));

    let mut events = Vec::new();
    match response {
        Ok(Response::Ready) => {
            this.borrow_mut().ready = true;
            events.push(Event::Ready);
        }
        Ok(Response::Rendered { id, result, millis }) => {
            let mut inner = this.borrow_mut();
            // Answers to requests abandoned by a restart are ignored.
            if inner
                .in_flight
                .as_ref()
                .is_some_and(|flight| flight.id == id)
                && let Some(InFlight { program, .. }) = inner.in_flight.take()
            {
                events.push(Event::Rendered {
                    program,
                    result,
                    millis,
                });
                if inner.pending.is_none() {
                    events.push(Event::Ready);
                }
            }
        }
        Err(message) => {
            let weak = Rc::downgrade(this);
            defer(&weak, move |this| {
                restart(
                    this,
                    Event::Crashed(format!("bad message from worker: {message}")),
                );
            });
            return;
        }
    }
    events.extend(pump(this));
    emit(this, events);
}

/// Sends the pending program if the worker is free.
fn pump(this: &Rc<RefCell<Inner>>) -> Vec<Event> {
    let mut inner = this.borrow_mut();
    if !inner.ready || inner.in_flight.is_some() {
        return Vec::new();
    }
    let Some(program) = inner.pending.take() else {
        return Vec::new();
    };
    let Some(connection) = &inner.connection else {
        return Vec::new();
    };

    let id = inner.next_id;
    let request = Request { id, program };
    if let Err(error) = connection.worker.post_message(&request.to_json().into()) {
        return vec![Event::Crashed(format!("cannot reach worker: {error:?}"))];
    }

    let weak = Rc::downgrade(this);
    let deadline = Timeout::new(inner.timeout_ms, move || {
        defer(&weak, |this| restart(this, Event::TimedOut));
    });
    inner.next_id += 1;
    inner.in_flight = Some(InFlight {
        id,
        program: request.program,
        _deadline: deadline,
    });
    vec![Event::Busy]
}

fn emit(this: &Rc<RefCell<Inner>>, events: Vec<Event>) {
    let on_event = Rc::clone(&this.borrow().on_event);
    for event in events {
        on_event(event);
    }
}

/// Runs `action` after the current JS callback has returned.
fn defer(weak: &Weak<RefCell<Inner>>, action: impl FnOnce(&Rc<RefCell<Inner>>) + 'static) {
    let weak = weak.clone();
    spawn_local(async move {
        if let Some(this) = weak.upgrade() {
            action(&this);
        }
    });
}
