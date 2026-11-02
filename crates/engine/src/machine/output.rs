//! Captures what Prolog writes to its output streams.

use std::cell::{Cell, RefCell};
use std::io::Cursor;
use std::rc::Rc;

/// Scryer's stream callback type (not re-exported by the crate).
type StreamCallback = Box<dyn FnMut(&mut Cursor<Vec<u8>>)>;

/// A shared, optionally size-limited buffer fed by Scryer stream callbacks.
#[derive(Debug, Clone)]
pub(crate) struct Capture {
    buffer: Rc<RefCell<Vec<u8>>>,
    limit: Rc<Cell<usize>>,
    truncated: Rc<Cell<bool>>,
}

impl Capture {
    /// A capture that keeps at most `limit` bytes until [`Self::set_limit`].
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            buffer: Rc::default(),
            limit: Rc::new(Cell::new(limit)),
            truncated: Rc::default(),
        }
    }

    /// A stream callback that drains the stream's buffer into this capture.
    pub(crate) fn callback(&self) -> StreamCallback {
        let capture = self.clone();
        Box::new(move |cursor| {
            let mut buffer = capture.buffer.borrow_mut();
            let room = capture.limit.get().saturating_sub(buffer.len());
            let pending = cursor.get_ref();
            if pending.len() > room {
                capture.truncated.set(true);
            }
            buffer.extend_from_slice(&pending[..pending.len().min(room)]);
            cursor.get_mut().clear();
            cursor.set_position(0);
        })
    }

    pub(crate) fn clear(&self) {
        self.buffer.borrow_mut().clear();
        self.truncated.set(false);
    }

    pub(crate) fn set_limit(&self, limit: usize) {
        self.limit.set(limit);
    }

    /// Takes everything captured so far, noting a truncation at the end.
    pub(crate) fn take(&self) -> String {
        let bytes = std::mem::take(&mut *self.buffer.borrow_mut());
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        if self.truncated.replace(false) {
            text.push_str("\n… output truncated");
        }
        text
    }
}
