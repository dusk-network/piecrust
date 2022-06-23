use core::panic::PanicInfo;

#[alloc_error_handler]
fn foo(_: core::alloc::Layout) -> ! {
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}
