use crate::boxed::Box;

pub struct Vec<T> {
    #[allow(unused)]
    len: usize,
    #[allow(unused)]
    capacity: usize,
    #[allow(unused)]
    backing: Box<[T]>,
}

impl<T> Vec<T> {
    pub const fn new() -> Self {
        Vec {
            len: 0,
            capacity: 0,
            backing: Box::empty_slice(),
        }
    }

    pub fn push(&mut self, _t: T) {
        loop {}
    }

    pub fn pop(&mut self) -> Option<T> {
        loop {}
    }
}
