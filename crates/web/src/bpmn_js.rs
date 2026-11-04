//! Bindings to the bpmn-js navigated viewer, loaded by `index.html` as the
//! global `BpmnJS`.

use js_sys::{Function, Object, Promise, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlElement;

#[wasm_bindgen]
extern "C" {
    /// `BpmnJS` from `bpmn-navigated-viewer.production.min.js`.
    #[wasm_bindgen(js_name = BpmnJS)]
    #[derive(Debug, Clone)]
    pub type Viewer;

    #[wasm_bindgen(constructor, js_class = "BpmnJS", catch)]
    fn construct(options: &Object) -> Result<Viewer, JsValue>;

    #[wasm_bindgen(method, js_name = importXML)]
    fn import_xml(this: &Viewer, xml: &str) -> Promise;

    #[wasm_bindgen(method)]
    fn get(this: &Viewer, service: &str) -> JsValue;

    #[wasm_bindgen(method)]
    fn destroy(this: &Viewer);

    /// diagram-js' `eventBus` service.
    #[derive(Debug, Clone)]
    type EventBus;

    #[wasm_bindgen(method)]
    fn on(this: &EventBus, event: &str, callback: &Function);

    /// diagram-js' `canvas` service.
    #[derive(Debug, Clone)]
    type Canvas;

    #[wasm_bindgen(method, js_name = zoom)]
    fn zoom_to(this: &Canvas, mode: &str);

    #[wasm_bindgen(method, js_name = viewbox)]
    fn get_viewbox(this: &Canvas) -> JsValue;

    #[wasm_bindgen(method, js_name = viewbox)]
    fn set_viewbox(this: &Canvas, viewbox: &JsValue);

    #[wasm_bindgen(method, catch, js_name = addMarker)]
    fn add_marker(this: &Canvas, element: &str, marker: &str) -> Result<(), JsValue>;

    /// diagram-js' `zoomScroll` service.
    #[derive(Debug, Clone)]
    type ZoomScroll;

    #[wasm_bindgen(method, js_name = stepZoom)]
    fn step_zoom(this: &ZoomScroll, delta: f64);
}

impl Viewer {
    /// Creates a viewer that draws into `container`.
    ///
    /// # Errors
    ///
    /// Fails if bpmn-js did not load or rejects the container.
    pub fn new(container: &HtmlElement) -> Result<Self, JsValue> {
        let options = Object::new();
        Reflect::set(&options, &"container".into(), container)?;
        Self::construct(&options)
    }

    /// Shows `xml`, keeping the current viewport unless `fit` asks to fit the
    /// diagram into it, and marks `flagged` elements with the CSS class `marker`.
    ///
    /// Unknown element ids in `flagged` are ignored.
    ///
    /// # Errors
    ///
    /// Returns bpmn-js' message if the XML cannot be imported.
    pub async fn show(
        &self,
        xml: &str,
        flagged: &[String],
        marker: &str,
        fit: bool,
    ) -> Result<(), String> {
        let canvas = self.canvas();
        let viewbox = (!fit).then(|| canvas.get_viewbox());

        JsFuture::from(self.import_xml(xml))
            .await
            .map_err(|error| {
                Reflect::get(&error, &"message".into())
                    .ok()
                    .and_then(|message| message.as_string())
                    .unwrap_or_else(|| format!("{error:?}"))
            })?;

        match viewbox {
            Some(viewbox) => canvas.set_viewbox(&viewbox),
            None => canvas.zoom_to("fit-viewport"),
        }
        for id in flagged {
            // bpmn-js throws for ids it does not know, e.g. dangling flow ends.
            let _ = canvas.add_marker(id, marker);
        }
        Ok(())
    }

    /// Calls `handler` with the id of every double-clicked element, a label
    /// counting as its element. The canvas itself is no element. The listener
    /// lives as long as the returned closure.
    pub fn on_double_click(
        &self,
        handler: impl Fn(String) + 'static,
    ) -> Closure<dyn FnMut(JsValue)> {
        let listener = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
            if let Some(id) = double_clicked_id(&event) {
                handler(id);
            }
        });
        let bus: EventBus = self.get("eventBus").unchecked_into();
        bus.on("element.dblclick", listener.as_ref().unchecked_ref());
        listener
    }

    /// Zooms in by one step around the centre of the viewport.
    pub fn zoom_in(&self) {
        self.zoom_scroll().step_zoom(1.0);
    }

    /// Zooms out by one step around the centre of the viewport.
    pub fn zoom_out(&self) {
        self.zoom_scroll().step_zoom(-1.0);
    }

    /// Fits the diagram into the viewport.
    pub fn fit(&self) {
        self.canvas().zoom_to("fit-viewport");
    }

    /// Releases the viewer's DOM and listeners.
    pub fn dispose(&self) {
        self.destroy();
    }

    fn canvas(&self) -> Canvas {
        self.get("canvas").unchecked_into()
    }

    fn zoom_scroll(&self) -> ZoomScroll {
        self.get("zoomScroll").unchecked_into()
    }
}

/// The id of the element behind an `element.dblclick` event, unless it is the
/// root, which has no parent.
fn double_clicked_id(event: &JsValue) -> Option<String> {
    let property = |object: &JsValue, name: &str| {
        Reflect::get(object, &name.into())
            .ok()
            .filter(|value| !value.is_undefined() && !value.is_null())
    };
    let element = property(event, "element")?;
    property(&element, "parent")?;
    let element = property(&element, "labelTarget").unwrap_or(element);
    property(&element, "id")?.as_string()
}
