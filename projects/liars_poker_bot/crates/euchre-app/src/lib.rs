use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

pub mod in_game;
pub mod requests;
pub mod settings;

pub const SERVER: &str = "api";
pub const ACTION_BUTTON_CLASS: &str = "bg-white outline outline-black hover:bg-slate-100 focus:outline-none focus:ring focus:bg-slate-100 active:bg-slate-200 rounded-lg disabled:outline-white";

struct PlayerId {
    id: usize,
}

impl From<PlayerId> for usize {
    fn from(value: PlayerId) -> Self {
        value.id
    }
}

pub fn player_id<T>(cx: Scope<T>) -> Option<usize> {
    use_shared_state::<PlayerId>(cx).map(|x| x.read().id)
}

pub fn set_player_id(cx: Scope, id: usize) {
    use_shared_state_provider(cx, || PlayerId { id });
}

struct EventId {
    id: String,
}

/// Returns the current event id if one has been set
pub fn event_id<T>(cx: Scope<T>) -> Option<String> {
    use_shared_state::<EventId>(cx).map(|x| x.read().id.clone())
}

pub fn set_event_id<T>(cx: Scope<T>, id: String) {
    use_shared_state_provider(cx, || EventId { id });
}

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
