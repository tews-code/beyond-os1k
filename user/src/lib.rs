//! User library for os1k
//!
//! System calls for os1k user processes.

#![no_std]

use core::arch::{asm, naked_asm};
use core::panic::PanicInfo;

pub use common::{print, println};

use common::{
    SYS_PUTBYTE,
    SYS_GETCHAR,
    SYS_EXIT,
    SYS_READFILE,
    SYS_WRITEFILE,
};

/// User panic handler
///
/// Prints a panic message and exits the process.
#[panic_handler]
pub fn panic(info: &PanicInfo) -> ! {
    println!("😬 User Panic! {}", info);
    exit();
}

unsafe extern "C" {
    static __user_stack_top: u8;
}

#[doc(hidden)]
pub fn sys_call(arg0: isize, arg1: isize, arg2: isize, arg3: isize, sysno: usize)  -> isize {
    let a0: isize;
    unsafe{asm!(
        "ecall",
        inout("a0") arg0 => a0,
        in("a1") arg1,
        in("a2") arg2,
        in("a3") arg3,
        in("a7") sysno,
    )}
    a0
}

/// Put a byte onto the debug console
///
/// Returns `Err` if the function fails.
/// Must be called repeatedly for each byte of a multibyte character.
#[unsafe(no_mangle)]
pub fn put_byte(b: u8) -> Result<(), isize> {
    let result = sys_call(b as isize, 0, 0, 0, SYS_PUTBYTE);
    if result == 0 {
        Ok(())
    } else {
        Err(result)
    }
}

/// Get character (or more accurately a byte) from the debug console
///
/// If no character is read, returns `None`.
///
/// Characters are returned as `usize` values. For multibyte characters, the function must be called for each byte.
///
/// Does not block.
pub fn get_char() -> Option<usize> {
    let ch = sys_call(0, 0, 0, 0, SYS_GETCHAR);
    if ch == -1 {
        None
    } else {
        Some(ch as usize)
    }
}


/// Exit the process
///
/// System call to exit the process immediately.
#[unsafe(no_mangle)]
pub fn exit() -> ! {
    let _ = sys_call(0, 0, 0, 0, SYS_EXIT);
    unreachable!("just in case!");
}

/// Read a text file from the file system
///
/// - `filename`: Complete file name as a Rust string slice
/// - `buf`: Byte buffer to receive the file contents
pub fn readfile(filename: &str, buf: &mut [u8]) {
    let _ = sys_call(filename.as_ptr() as isize, filename.len() as isize, buf.as_mut_ptr() as isize, buf.len() as isize, SYS_READFILE);
}

/// Write text to file
///
/// - `filename`: Complete file name as a Rust string slice
/// - `buf`: Byte buffer which will be written to the file
pub fn writefile(filename: &str, buf: &[u8]) {
    let _ = sys_call(filename.as_ptr() as isize, filename.len() as isize,  buf.as_ptr() as isize, buf.len() as isize, SYS_WRITEFILE);
}

#[unsafe(link_section = ".text.start")]
#[unsafe(no_mangle)]
#[unsafe(naked)]
unsafe extern "C" fn start() {
    naked_asm!(
        "la sp, {stack_top}",
        "call main",
        "call exit",
        stack_top = sym __user_stack_top
    )
}

