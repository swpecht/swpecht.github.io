#[macro_export]
macro_rules! counter {
    // This macro takes an argument of designator `ident` and
    // creates a function named `$func_name`.
    // The `ident` designator is used for variable/function names.
    ($name:ident) => {
        pub mod $name {
            use std::sync::atomic::AtomicUsize;

            static mut COUNTER: AtomicUsize = AtomicUsize::new(0);

            pub fn increment() {
                unsafe {
                    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }

            pub fn read() -> usize {
                unsafe { COUNTER.load(std::sync::atomic::Ordering::Relaxed) }
            }
        }
    };
}

#[macro_export]
macro_rules! guage {
    // This macro takes an argument of designator `ident` and
    // creates a function named `$func_name`.
    // The `ident` designator is used for variable/function names.
    ($name:ident) => {
        pub mod $name {
            use std::sync::atomic::AtomicUsize;

            static mut COUNTER: AtomicUsize = AtomicUsize::new(0);

            pub fn increment() {
                unsafe {
                    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }

            pub fn set(val: usize) {
                unsafe {
                    COUNTER.swap(val, std::sync::atomic::Ordering::Relaxed);
                }
            }

            pub fn read() -> usize {
                unsafe { COUNTER.load(std::sync::atomic::Ordering::Relaxed) }
            }
        }
    };
}
