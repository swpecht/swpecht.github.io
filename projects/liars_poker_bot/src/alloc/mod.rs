#[derive(Clone)]
pub struct Pool<T> {
    pool: Vec<T>,
    generator: fn() -> T,
}

impl<T> Pool<T> {
    pub fn new(generator: fn() -> T) -> Self {
        Self {
            pool: Vec::new(),
            generator,
        }
    }

    pub fn detach(&mut self) -> T {
        if self.pool.is_empty() {
            return (self.generator)();
        }

        self.pool.pop().unwrap()
    }

    pub fn attach(&mut self, obj: T) {
        self.pool.push(obj);
    }
}
