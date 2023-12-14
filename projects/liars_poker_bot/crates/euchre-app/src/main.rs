#![allow(non_snake_case)]

fn main() {
    // launch the web app
    wasm_logger::init(wasm_logger::Config::default());
    dioxus_web::launch(euchre_app::app::App);
}
