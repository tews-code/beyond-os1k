//! Kernel entry

use core::arch::naked_asm;

use crate::scheduler::SSTATUS_SIE;

#[unsafe(naked)]
pub unsafe extern "C" fn kernel_entry() {
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

        // Restore after trap handled

        // Disable interrupts atomically and check value
        "csrrci t0, sstatus, {sstatus_sie}",

        "lw a0, 31 * 4(sp)",            // Load stored sscratch value into temp register
        "csrw sscratch, a0",            // Restore sscratch to before trap

        "lw ra,  4 *  0(sp)",
        "lw gp,  4 *  1(sp)",
        "lw tp,  4 *  2(sp)",
        // "lw t0,  4 *  3(sp)",        // t0 temp holding interrupt status
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

        // Re-enable interrupts if they were enabled
        "beqz t0, 4f",
        "lw t0,  4 *  3(sp)",        // Restore t0
        "lw sp,  4 * 30(sp)",
        "csrsi sstatus, {sstatus_sie}",
        "j 5f",

        "4:",
        "lw t0,  4 *  3(sp)",        // Restore t0
        "lw sp,  4 * 30(sp)",

        "5:",
        "sret",
        sstatus_sie = const SSTATUS_SIE,
    );
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
