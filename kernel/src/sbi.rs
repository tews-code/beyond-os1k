//! SBI Interface

use core::arch::asm;
use core::ffi::{c_long, c_int};

pub const EID_SET_TIMER: c_long = 0;
pub const EID_CONSOLE_PUTCHAR: c_long = 1;
pub const EID_CONSOLE_GETCHAR: c_long = 2;

#[unsafe(no_mangle)]
pub fn put_byte(b: u8) -> Result<isize, isize> {
    let result: c_long;
    unsafe {
        asm!(
            "ecall",
             inlateout("a0") b as c_int => result,
             in("a7") EID_CONSOLE_PUTCHAR,
        );
    }
    if result == 0 {
        Ok(0)
    } else {
        Err(result as isize)
    }
}

pub fn get_char() -> Result<isize, isize> {
    let result: c_long;
    unsafe {
        asm!(
            "ecall",
             out("a0") result,
             in("a7") EID_CONSOLE_GETCHAR,
        );
    }
    if result != -1 {
        Ok(result as isize)
    } else {
        Err(-1)
    }
}

pub fn set_timer(ticks: u64) -> Result<isize, isize> {
    let result: c_long;
    let ticksl = (ticks & 0xFFFFFFFF) as u32;
    let ticksh = (ticks >> 32) as u32;
    unsafe {
        asm!(
            "ecall",
             inlateout("a0") ticksl => result,
             in("a1") ticksh,
             in("a7") EID_SET_TIMER,
             options(nomem, nostack)
        );
    }
    if result == 0 {
        Ok(result as isize)
    } else {
        Err(result as isize)
    }
}
