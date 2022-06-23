struct Allocation {
    backing: alloc::boxed::Box<[u8]>,
}

struct Host {
    map: BTreeMap<*mut u8, Allocation>,
}
