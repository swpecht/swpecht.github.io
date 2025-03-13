use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, signal::Signal};

pub static SHARED: Signal<ThreadModeRawMutex, u8> = Signal::new();
