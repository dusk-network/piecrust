mod ext {
    extern "C" {
        pub(crate) fn snap();
    }
}

pub fn snap() {
    unsafe { ext::snap() };
}
