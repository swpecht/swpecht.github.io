use std::marker::PhantomData;

struct Bump<T> {
    mem: Vec<T>,
}

struct BumpVec<T> {
    ptr: u8,
    len: u8,
    cap: u8,
    _phantom: PhantomData<T>,
}
