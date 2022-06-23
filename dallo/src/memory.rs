use core::mem::MaybeUninit;

pub(crate) struct Memory;

impl Memory {
    pub fn alloc<T>(n: usize) -> MaybeUninit<*mut T> {
        {
            #[cfg(feature = "host")]
            crate::host_mem::HostMem::alloc(n)
        }
        {
            #[cfg(not(feature = "host"))]
            crate::guest_mem::GuestMem::alloc(n)
        }
    }

    pub fn dealloc<T: ?Sized>(ptr: *mut T) {
        let byteptr: *mut u8 = ptr.cast();
        {
            #[cfg(feature = "host")]
            crate::host_mem::HostMem::dealloc(byteptr)
        }
        {
            #[cfg(not(feature = "host"))]
            crate::guest_mem::GuestMem::dealloc(byteptr)
        }
    }
}
