//! OS in 1000 lines

#![no_std]
#![no_main]

pub extern crate alloc;

use core::arch::{
    asm,
    naked_asm,
};
use core::ptr::write_bytes;

#[allow(unused_imports)]
use common::{print, println};

mod address;
mod allocator;
#[macro_use]
mod entry;
mod page;
mod panic;
mod process;
mod tar;
mod trap;
mod sbi;
mod scheduler;
mod spinlock;
mod timer;
mod virtio;

use crate::entry::kernel_entry;
use crate::process::{create_process,user_entry};
use crate::scheduler::{scheduler_init, yield_now};
use crate::tar::fs_init;
use crate::virtio::virtio_blk_init;

unsafe extern "C" {
    // Safety: Symbols created by linker script
    static __bss: u8;
    static __bss_end: u8;
    static __stack_top: u8;
}

unsafe extern "C" {
    // Safety: Symbols created by linker script
    static _binary_shell_bin_start: u8;
    static _binary_shell_bin_size: u8;
}

fn delay() {
    for _ in 0..300_000_000usize {
        unsafe{asm!("nop");} // do nothing
    }
}

fn proc_a_entry() {
    println!("starting process A");
    loop {
        print!("🐈");
        delay();
    }
}

fn proc_b_entry() {
    println!("starting process B");
    loop {
        print!("🐕");
        delay();
    }
}

#[unsafe(no_mangle)]
fn kernel_main() -> ! {
    let bss = &raw const __bss;
    let bss_end = &raw const __bss_end;
    unsafe {
        // Safety: from linker script bss is aligned and bss segment is valid for writes up to bss_end
        write_bytes(bss as *mut u8, 0, bss_end as usize - bss as usize);
    }

    write_csr!("stvec", kernel_entry as usize);

    common::println!("Hello World!\n🦀 initialising ...");
    virtio_blk_init();
    fs_init();
    scheduler_init();

    let _ = create_process(proc_a_entry as usize, core::ptr::null(), 0);
    let _ = create_process(proc_b_entry as usize, core::ptr::null(), 0);

    let shell_start = &raw const _binary_shell_bin_start as *mut u8;
    let shell_size = &raw const _binary_shell_bin_size as usize;  // The symbol _address_ is the size of the binary
    let _ = create_process(user_entry as *const() as usize, shell_start, shell_size);

    yield_now();

    unreachable!("should never reach here!");
}

#[unsafe(link_section = ".text.boot")]
#[unsafe(no_mangle)]
#[unsafe(naked)]
unsafe extern "C" fn boot() -> ! {
    naked_asm!(
        "la a0, {stack_top}",
        "mv sp, a0",
        "j {kernel_main}",
        stack_top = sym __stack_top,
        kernel_main = sym kernel_main,
    );
}
