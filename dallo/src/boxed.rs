use core::ops::{Deref, DerefMut};

use crate::memory::Memory;

pub struct Box<T: ?Sized> {
    ptr: *mut T,
}

impl<T: ?Sized> Drop for Box<T> {
    fn drop(&mut self) {
        Memory::dealloc(self.ptr)
    }
}

impl<T> Box<T> {
    pub fn new(mut t: T) -> Self {
        let mut ptr = Memory::alloc(1);
        ptr.write(&mut t);
        let ptr = unsafe { ptr.assume_init() };
        Box { ptr }
    }

    pub const fn empty_slice() -> Box<[T]> {
        let m: &mut [T] = &mut [];
        Box { ptr: m }
    }
}

impl<T> Deref for Box<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

impl<T> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.ptr }
    }
}
