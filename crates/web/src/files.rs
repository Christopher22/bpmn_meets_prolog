//! Handing files to the user.

use gloo_timers::callback::Timeout;
use leptos::prelude::document;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

/// What the file picker offers when opening a program.
pub(crate) const PROLOG_EXTENSIONS: &str = ".pl,.pro,.prolog";

/// The media type of saved programs.
pub(crate) const PROLOG_MIME: &str = "text/plain;charset=utf-8";

/// The media type of saved BPMN documents.
pub(crate) const BPMN_MIME: &str = "application/xml;charset=utf-8";

/// How long a download's URL stays valid; browsers may fetch it asynchronously.
const REVOKE_AFTER_MS: u32 = 10_000;

/// Offers `content` as a download called `name`.
pub(crate) fn download(name: &str, mime: &str, content: &str) -> Result<(), JsValue> {
    let parts = js_sys::Array::of1(&JsValue::from_str(content));
    let options = BlobPropertyBag::new();
    options.set_type(mime);
    let blob = Blob::new_with_str_sequence_and_options(&parts, &options)?;
    let url = Url::create_object_url_with_blob(&blob)?;

    let anchor: HtmlAnchorElement = document().create_element("a")?.unchecked_into();
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();

    Timeout::new(REVOKE_AFTER_MS, move || {
        let _ = Url::revoke_object_url(&url);
    })
    .forget();
    Ok(())
}

/// The file name without its extension, unless nothing would be left.
pub(crate) fn stem(name: &str) -> Option<&str> {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    (!stem.is_empty()).then_some(stem)
}

#[cfg(test)]
mod tests {
    use super::stem;

    #[test]
    fn stems() {
        assert_eq!(stem("it_order.pl"), Some("it_order"));
        assert_eq!(stem("a.b.pl"), Some("a.b"));
        assert_eq!(stem("order"), Some("order"));
        assert_eq!(stem(".pl"), None);
        assert_eq!(stem(""), None);
    }
}
