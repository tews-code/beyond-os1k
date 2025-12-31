//! Kernel entry

use alloc::slice;
use core::arch::naked_asm;

use common::{
    SYS_PUTBYTE,
    SYS_GETCHAR,
    SYS_EXIT,
    SYS_READFILE,
    SYS_WRITEFILE,
};

use crate::process::{PROCS, State};
use crate::sbi::{put_byte, get_char};
use crate::scheduler::{yield_now, CURRENT_PROC};
use crate::tar::{FILES, fs_flush};
use crate::timer::TIMER;
use crate::{println, read_csr, write_csr};

const SCAUSE_ECALL: usize = 8;
const SCAUSE_TIMER_INTERRUPT: usize = 0x80000005;

#[derive(Debug)]
#[repr(C, packed)]
pub struct TrapFrame{
    ra: usize,      // 0
    gp: usize,
    tp: usize,
    t0: usize,
    t1: usize,
    t2: usize,
    t3: usize,
    t4: usize,
    t5: usize,
    t6: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
    a7: usize,
    s0: usize,
    s1: usize,
    s2: usize,
    s3: usize,
    s4: usize,
    s5: usize,
    s6: usize,
    s7: usize,
    s8: usize,
    s9: usize,
    s10: usize,
    s11: usize,
    sp: usize,          // 30
    sscratch: usize,    // 31
}

#[unsafe(naked)]
pub unsafe extern "C" fn kernel_entry() -> ! {
    naked_asm!(
        ".align 2",

        // Retrieve the kernel stack of the running process from sscratch.
        "csrrw sp, sscratch, sp",       // Swap sp and sscratch

        // Check if this is process has trapped from userspace (sscratch != zero)
        "bnez sp, 1f",
        "csrr sp, sscratch",            // Get kernel sp back from sscratch

        "1:",
        "addi sp, sp, -4 * 32",
        "sw ra,  4 * 0(sp)",
        "sw gp,  4 * 1(sp)",
        "sw tp,  4 * 2(sp)",
        "sw t0,  4 * 3(sp)",
        "sw t1,  4 * 4(sp)",
        "sw t2,  4 * 5(sp)",
        "sw t3,  4 * 6(sp)",
        "sw t4,  4 * 7(sp)",
        "sw t5,  4 * 8(sp)",
        "sw t6,  4 * 9(sp)",
        "sw a0,  4 * 10(sp)",
        "sw a1,  4 * 11(sp)",
        "sw a2,  4 * 12(sp)",
        "sw a3,  4 * 13(sp)",
        "sw a4,  4 * 14(sp)",
        "sw a5,  4 * 15(sp)",
        "sw a6,  4 * 16(sp)",
        "sw a7,  4 * 17(sp)",
        "sw s0,  4 * 18(sp)",
        "sw s1,  4 * 19(sp)",
        "sw s2,  4 * 20(sp)",
        "sw s3,  4 * 21(sp)",
        "sw s4,  4 * 22(sp)",
        "sw s5,  4 * 23(sp)",
        "sw s6,  4 * 24(sp)",
        "sw s7,  4 * 25(sp)",
        "sw s8,  4 * 26(sp)",
        "sw s9,  4 * 27(sp)",
        "sw s10, 4 * 28(sp)",
        "sw s11, 4 * 29(sp)",

        // Retrieve and save the sp at the time of exception
        "csrr a0, sscratch",        // Load sscratch into a0 (which is already stored)
        "bnez a0, 2f",              // Check if sscratch is non zero (user process)

        // Kernel process
        "sw sp, 4 * 30(sp)",        // Kernel process using already the actual stack pointer
        "sw zero, 4 * 31(sp)",      // Kernel process sscratch stored as zero
        "j 3f",

        // User process
        "2:",
        "sw a0, 4 * 30(sp)",        // User process, have just loaded stack pointer into a0
        "addi a0, sp, 4 * 32",      // a0 = sp + trap frame which is kernel stack top
        "sw a0, 4 * 31(sp)",

        "3:",
        // Now set sscratch to zero for kernel space
        "csrw sscratch, x0",            // Zero sscratch now we are in kernel space
        "mv a0, sp",                // a0 is set to sp (bottom of trap frame)
        "call handle_trap",
    );
}

#[unsafe(naked)]
extern "C" fn kernel_return(f: &mut TrapFrame) -> ! {
    naked_asm!(
        "mv sp, a0",                // Trap frame is in a0, switch this to sp

        // Check if we trapped from userspace and restore sscratch
        // "csrr a0, sstatus",         // Read sstatus to a0 (safe as a0 saved to sp)
        // "andi a0, a0, 8",           // sstatus.SPP is at bit 8 (0=prev priv user)
        // "bnez a0, 1f",
        // "li a0,"
        // "1:",
        // "csrw a0, sscratch",
        // Now restore frame
        "lw a0, 31 * 4(sp)",            // Load stored sscratch value into temp register
        "csrw sscratch, a0",            // Restore sscratch to before trap

        "lw ra,  4 *  0(sp)",
        "lw gp,  4 *  1(sp)",
        "lw tp,  4 *  2(sp)",
        "lw t0,  4 *  3(sp)",
        "lw t1,  4 *  4(sp)",
        "lw t2,  4 *  5(sp)",
        "lw t3,  4 *  6(sp)",
        "lw t4,  4 *  7(sp)",
        "lw t5,  4 *  8(sp)",
        "lw t6,  4 *  9(sp)",
        "lw a0,  4 * 10(sp)",       // a0 from before trap is restored here
        "lw a1,  4 * 11(sp)",
        "lw a2,  4 * 12(sp)",
        "lw a3,  4 * 13(sp)",
        "lw a4,  4 * 14(sp)",
        "lw a5,  4 * 15(sp)",
        "lw a6,  4 * 16(sp)",
        "lw a7,  4 * 17(sp)",
        "lw s0,  4 * 18(sp)",
        "lw s1,  4 * 19(sp)",
        "lw s2,  4 * 20(sp)",
        "lw s3,  4 * 21(sp)",
        "lw s4,  4 * 22(sp)",
        "lw s5,  4 * 23(sp)",
        "lw s6,  4 * 24(sp)",
        "lw s7,  4 * 25(sp)",
        "lw s8,  4 * 26(sp)",
        "lw s9,  4 * 27(sp)",
        "lw s10, 4 * 28(sp)",
        "lw s11, 4 * 29(sp)",
        "lw sp,  4 * 30(sp)",

        "sret"
    );
}

#[unsafe(no_mangle)]
extern "C" fn handle_trap(f: &mut TrapFrame) -> ! {
    let scause = read_csr!("scause");
    let stval = read_csr!("stval");
    let mut user_pc = read_csr!("sepc");

    // crate::println!("in handle_trap, sie is {:032b}", read_csr!("sie"));
    // crate::println!("in handle_trap, sstatus is {:032b}", read_csr!("sstatus"));
    // Reenable interrupts
    // let mut sstatus = read_csr!("sstatus");
    // sstatus |= crate::process::SSTATUS_SIE;
    // write_csr!("sstatus", sstatus);

    // crate::println!("in handle_trap, sstatus updated to {:032b}", read_csr!("sstatus"));
    // let sscratch = read_csr!("sscratch");
    // crate::println!("in handle_trap, sscratch is {sscratch:x}");

    if scause == SCAUSE_ECALL {
        unsafe {
            core::arch::asm!("csrsi sstatus, 0x2");
        }
        handle_syscall(f);
        user_pc += 4;
        write_csr!("sepc", user_pc);
    } else if scause == SCAUSE_TIMER_INTERRUPT {
        println!("Timer interrupt!");
        // println!("Trap frame is {f:x?}");
        TIMER.set(500);
        unsafe {
            core::arch::asm!("csrsi sstatus, 0x2");
        }
        // crate::println!("timer interrupt: trap frame {f:x?}");
        // crate::println!("sepc is {:x}", read_csr!("sepc"));
        let current_pid = CURRENT_PROC.lock()
            .expect("current proc should be initialised");
        let next_pid = PROCS.get_next(current_pid);
        // let _frame = PROCS.try_get_frame(next_pid);
        yield_now();
        kernel_return(f);
    } else {
        panic!("unexpected trap scause=0x{:x}, stval=0x{:x}, sepc=0x{:x}", scause, stval, user_pc);
    }

    // crate::println!("in handle_trap, frame is {f:x?}");
    kernel_return(f);
}

fn handle_syscall(f: &mut TrapFrame) {
    let sysno = f.a4;
    match sysno {
        SYS_PUTBYTE => {  // Match what user code sends
            match put_byte(f.a0 as u8) {
                Ok(_) => f.a0 = 0,     // Set return value to 0 (success)
                Err(e) => f.a0 = e as usize,    // Set return value to error code
            }
        },
        SYS_GETCHAR => {
            loop {
                if let Ok(ch) = get_char() {
                    f.a0 = ch as usize;
                    break;
                }
                crate::println!("in sys_getchar");
                yield_now();
            }
        },
        SYS_EXIT => {
            let current = CURRENT_PROC.lock()
                .expect("current process should be running");
            crate::println!("process {} exited", current);
            if let Some(p) = PROCS.0.lock().iter_mut()
                .find(|p| p.pid == current) {
                    p.state = State::Exited
                }
            yield_now();
            unreachable!("unreachable after SYS_EXIT");
        },
        SYS_READFILE | SYS_WRITEFILE => 'block: {
            let filename_ptr = f.a0 as *const u8;
            let filename_len = f.a1;

            // Safety: Caller guarantees that filename_ptr points to valid memory
            // of length filename_len that remains valid for the lifetime of this reference
            let filename = unsafe {
                str::from_utf8(slice::from_raw_parts(filename_ptr, filename_len))
            }.expect("filename must be valid UTF-8");

            let buf_ptr = f.a2 as *mut u8;
            let buf_len = f.a3;

            // Safety: Caller guarantees that buf_ptr points to valid memory
            // of length buf_len that remains valid for the lifetime of this reference
            let buf = unsafe {
                slice::from_raw_parts_mut(buf_ptr, buf_len)
            };

            // println!("handling syscall SYS_READFILE | SYS_WRITEFILE for file {:?}", filename);

            let Some(file_i) = FILES.fs_lookup(filename) else {
                println!("file not found {:x?}", filename);
                f.a0 = usize::MAX; // 2's complement is -1
                break 'block;
            };

            match sysno {
                SYS_WRITEFILE => {
                    let mut files = FILES.0.lock();
                    // try_borrow_mut()
                    // .expect("should be able to borrow FILES mutably to handle SYS_WRITEFILE");

                    files[file_i].data[..buf.len()].copy_from_slice(buf);
                    files[file_i].size = buf.len();
                    drop(files);
                    fs_flush();
                },
                SYS_READFILE => {
                    let files = FILES.0.lock();
                    // try_borrow()
                    // .expect("should be able to borrow FILES to handle SYS_READFILE");

                    buf.copy_from_slice(&files[file_i].data[..buf.len()]);
                },
                _ => unreachable!("sysno must be SYS_READFILE or SYS_WRITEFILE"),
            }

            f.a0 = buf_len;
        },
        _ => {panic!("unexpected syscall sysno={:x}", sysno);},
    }
}

#[macro_export]
macro_rules! read_csr {
    ( $reg:literal ) => {
        {
            let val: usize;
            unsafe{core::arch::asm!(concat!("csrr {}, ", $reg), out(reg) val)}
            val
        }
    };
}

#[macro_export]
macro_rules! write_csr {
    ( $reg:literal, $val:expr ) => {
        {
            let val = $val; // Expand metavariable outside of unsafe block (avoids clippy warning)
            unsafe{core::arch::asm!(concat!("csrw ", $reg, ", {}"), in(reg) val)}
        }
    };
}
