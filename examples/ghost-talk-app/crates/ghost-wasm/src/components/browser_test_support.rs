use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::JsCast;
use web_sys::{Element, Event, HtmlElement, HtmlInputElement};

pub(crate) fn test_root() -> Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let root = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&root).unwrap();
    root
}

pub(crate) fn click(root: &Element, selector: &str) {
    root.query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("missing browser-test control: {selector}"))
        .dyn_into::<HtmlElement>()
        .unwrap()
        .click();
}

pub(crate) fn input(root: &Element, selector: &str, value: &str) {
    let input = root
        .query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("missing browser-test input: {selector}"))
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    input.set_value(value);
    input.dispatch_event(&Event::new("input").unwrap()).unwrap();
}

pub(crate) async fn settle() {
    TimeoutFuture::new(0).await;
    TimeoutFuture::new(0).await;
}
