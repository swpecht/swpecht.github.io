#[macro_export]
macro_rules! counter {
    // This macro takes an argument of designator `ident` and
    // creates a function named `$func_name`.
    // The `ident` designator is used for variable/function names.
    ($name:ident) => {
        pub mod $name {
            use std::sync::atomic::{AtomicUsize, Ordering};

            static COUNTER: AtomicUsize = AtomicUsize::new(0);

            pub fn increment() {
                COUNTER.fetch_add(1, Ordering::Relaxed);
            }

            pub fn read() -> usize {
                COUNTER.load(Ordering::Relaxed)
            }
        }
    };
}
