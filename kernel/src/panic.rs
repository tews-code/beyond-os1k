//! Panic for os1k

use core::arch::asm;
use core::panic::PanicInfo;

use crate::println;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("⚠️ Panic: {}", info);

    // Disable interrupts
    write_csr!("sstatus", 0);

    loop {
        unsafe {asm!("wfi")};
    }
}
