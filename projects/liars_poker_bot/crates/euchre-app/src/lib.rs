pub mod in_game;
pub mod requests;

pub const SERVER: &str = "api";
pub const ACTION_BUTTON_CLASS: &str = "bg-white outline outline-black hover:bg-slate-100 focus:outline-none focus:ring focus:bg-slate-100 active:bg-slate-200 py-1 rounded-lg disabled:outline-white";

pub struct PlayerId {
    pub id: usize,
}

pub fn base_url() -> String {
    web_sys::window().unwrap().location().origin().unwrap()
}
