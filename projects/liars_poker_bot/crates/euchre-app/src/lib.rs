use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

pub mod app;
pub mod in_game;
pub mod requests;
pub mod settings;

pub const SERVER: &str = "api";
pub const ACTION_BUTTON_CLASS: &str = "bg-white outline outline-black hover:bg-slate-100 focus:outline-none focus:ring focus:bg-slate-100 active:bg-slate-200 rounded-lg disabled:outline-white";

pub fn base_url() -> String {
    web_sys::window().unwrap().location().origin().unwrap()
}

pub fn hide_element(element_id: &str) {
    let window = web_sys::window().expect("should have a window in this context");
    let document = window.document().expect("window should have a document");

    document
        .get_element_by_id(element_id)
        .expect("should have #loading on the page")
        .dyn_ref::<HtmlElement>()
        .expect("#loading should be an `HtmlElement`")
        .style()
        .set_property("display", "none")
        .unwrap();
}

pub fn show_element(element_id: &str) {
    let window = web_sys::window().expect("should have a window in this context");
    let document = window.document().expect("window should have a document");

    document
        .get_element_by_id(element_id)
        .expect("should have #loading on the page")
        .dyn_ref::<HtmlElement>()
        .expect("#loading should be an `HtmlElement`")
        .style()
        .set_property("display", "block")
        .unwrap();
}
