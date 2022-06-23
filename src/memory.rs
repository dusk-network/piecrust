// super simple bump allocation proof of concept

#[derive(Debug)]
pub struct MemHandler {
    heap_base: usize,
}

impl MemHandler {
    pub fn new(heap_base: usize) -> Self {
        MemHandler { heap_base }
    }

    pub fn align_to(&mut self, n: usize) {
        self.heap_base += self.heap_base % n;
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> usize {
        self.align_to(align);
        let ofs = self.heap_base;
        self.heap_base += size;

        println!("allocating {} bytes at {}", size, ofs);

        ofs
    }
}
