//! Round-robin scheduler

use core::arch::naked_asm;

use crate::process::{create_process, Process, State};
use crate::spinlock::SpinLock;
use crate::timer::TIMER;

pub const PROCS_MAX: usize = 8;         // Maximum number of processes
pub struct Procs(pub SpinLock<[Process; PROCS_MAX]>);

impl Procs {
    const fn new() -> Self {
        Self(
            SpinLock::new([const { Process::zeroed() }; PROCS_MAX])
        )
    }

    pub fn try_get_index(&self, pid: usize) -> Option<usize> {
        self.0.lock().iter().position(|p| p.pid == pid)
    }

    pub fn get_next(&self, current_pid: usize) -> usize {
        // Search for the next runnable process; return IDLE_PID if none found
        let next_pid = {
            let current_index = PROCS.try_get_index(current_pid)
                .expect("current process PID should have an index");
            PROCS.0.lock().iter()
                .cycle()
                .skip(current_index + 1)
                .take(PROCS_MAX)
                .find(|p| p.state == State::Runnable && p.pid != IDLE_PID)
                .map(|p| p.pid)
                .unwrap_or(IDLE_PID)
        };
        next_pid
    }
}

pub static PROCS: Procs = Procs::new();  // All process control structures.

// Optional - but vital for debugging if you want to print the contents of PROCS.
// impl alloc::fmt::Display for Procs {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         let procs = PROCS.0.lock();
//         for (i, process) in procs.iter().enumerate() {
//             write!(f, "Addr: {:x?} ", &raw const *process as usize)?;
//             writeln!(f, "PROC[{i}]")?;
//             write!(f, "PID: {} ", process.pid)?;
//             write!(f, "SP: {:x?} ", process.sp)?;
//             writeln!(f, "STATE: {:?} ", process.state)?;
//             writeln!(f, "IS_KERNEL: {:?} ", process.is_kernel)?;
//             writeln!(f, "STACK:  ... {:x?}", &process.stack[process.stack.len()-128..process.stack.len()])? // Remember range top is _exclusive_ hence no panic
//         }
//         Ok(())
//     }
// }

pub static CURRENT_PROC: SpinLock<Option<usize>> = SpinLock::new(Some(IDLE_PID)); // Currently running process set to idle at start

pub const IDLE_PID: usize = 0;      // idle
const SIE_STIE: usize = 1 << 5;     // Enable supervisor timer interrupt
pub const SSTATUS_SIE: usize = 1 << 1;  // Enable supervisor interrupts
// const SSTATUS_SPIE: usize =  1 << 5;    // Supervisor previous interrupt state (enables interrupts on `sret`)
// const SSTATUS_SPP: usize = 1 << 8;      // Supervisor previous priv. level (user = 0, supervisor = 1)

fn idle_process() {
    panic!("reached idle process");
}

pub fn scheduler_init() {
    // Initialise idle process
    let idle_pid = create_process(idle_process as *const() as usize, core::ptr::null(), 0);
    if let Some(p) = PROCS.0.lock().iter_mut()
        .find(|p| p.pid == idle_pid) {
            p.pid = IDLE_PID;
        }

    // Enable timer interrupt in supervisor mode
    write_csr!("sie", SIE_STIE);                                    // Enable timer interrupt
    write_csr!("sstatus", read_csr!("sstatus") | SSTATUS_SIE);      // Enable all supervisor interrupts

    TIMER.set(500);                                                 // Scheduler interrupts at 500 ms
}

static FIRST_SWITCH: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);

pub fn yield_now() {
    let current_pid = CURRENT_PROC.lock()
        .expect("CURRENT_PROC initialised before use");

    // Search for a runnable process
    let next_pid = PROCS.get_next(current_pid);

    // If there's no runnable process other than the current one, return and continue processing
    if next_pid == current_pid {
        return;
    }

    let (next_sp_ptr, current_sp_ptr) = {
        let next_index = PROCS.try_get_index(next_pid)
            .expect("should find next by pid");
        let current_index = PROCS.try_get_index(current_pid)
            .expect("should find current by pid");
        let mut procs = PROCS.0.lock();
        let [next, current] = procs.get_disjoint_mut([next_index, current_index])
            .expect("indices should be valid and distinct");

        let next_sp_ptr = next.sp.field_raw_ptr();

        let current_sp_ptr = if FIRST_SWITCH.swap(false, core::sync::atomic::Ordering::Relaxed) {
            // First switch - provide a valid but ultimately discarded pointer
            &raw const next_sp_ptr as *mut usize  // Let's just reuse next_sp_ptr's address as we need any valid address and this will do
        } else {
            current.sp.field_raw_ptr()
        };

        (next_sp_ptr, current_sp_ptr)
    };

    // Context switch
    *CURRENT_PROC.lock() = Some(next_pid);
    unsafe {
        // Safety: Both stack pointers are valid pointers to runnable processes
        switch_context(current_sp_ptr, next_sp_ptr);
    }
}

#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(prev_sp: *mut usize, next_sp: *mut usize) {
    naked_asm!(
        ".align 2",

        // Atomically get current interrupt state and disable interrupts
        "csrrci t0, sstatus, {sstatus_sie}",

        // Save callee-saved registers onto the current process's stack.
        "addi sp, sp, -17 * 4", // Allocate stack space for 17 4-byte registers
        "sw ra,  0  * 4(sp)",  // Save callee-saved registers
        "sw s0,  1  * 4(sp)",
        "sw s1,  2  * 4(sp)",
        "sw s2,  3  * 4(sp)",
        "sw s3,  4  * 4(sp)",
        "sw s4,  5  * 4(sp)",
        "sw s5,  6  * 4(sp)",
        "sw s6,  7  * 4(sp)",
        "sw s7,  8  * 4(sp)",
        "sw s8,  9  * 4(sp)",
        "sw s9,  10 * 4(sp)",
        "sw s10, 11 * 4(sp)",
        "sw s11, 12 * 4(sp)",
        "csrr s0, sscratch",        // s0 is already stored, use as temp register to get current CSRs
        "sw s0, 13 * 4(sp)",
        "csrr s0, sepc",
        "sw s0, 14 * 4(sp)",
        "csrr s0, sstatus",
        "sw s0, 15 * 4(sp)",
        "csrr s0, satp",
        "sw s0, 16 * 4(sp)",

        // Switch the stack pointer using process.sp pointers
        "sw sp, (a0)",              // *prev_sp = sp;
        "lw sp, (a1)",              // Switch stack pointer (sp) here

        // Switch satp to next stack if different to current
        "lw s0, 16 * 4(sp)",
        "csrr s1, satp",
        "beq s0, s1, 1f",
        "csrw satp, s0",
        "sfence.vma",
        "1:",

        // Restore CSRs from the next process's stack.
        "lw s0, 13 * 4(sp)",
        "csrw sscratch, s0",        // Restore sscratch for next process
        "lw s0, 14 * 4(sp)",
        "csrw sepc, s0",
        "lw s0, 15 * 4(sp)",
        "csrw sstatus, s0",

        // Restore callee-saved registers from the next process's stack.
        "lw ra,  0  * 4(sp)",       // Restore callee-saved registers only
        "lw s0,  1  * 4(sp)",
        "lw s1,  2  * 4(sp)",
        "lw s2,  3  * 4(sp)",
        "lw s3,  4  * 4(sp)",
        "lw s4,  5  * 4(sp)",
        "lw s5,  6  * 4(sp)",
        "lw s6,  7  * 4(sp)",
        "lw s7,  8  * 4(sp)",
        "lw s8,  9  * 4(sp)",
        "lw s9,  10 * 4(sp)",
        "lw s10, 11 * 4(sp)",
        "lw s11, 12 * 4(sp)",
        "addi sp, sp, 17 * 4",              // We've popped 17 4-byte registers from the stack
        "beqz t0, 2f",                      // t0 = 0 means interrupts were disabled
        "csrsi sstatus, {sstatus_sie}",     // Reenable interrupts last thing

        "2:",
        "ret",
        sstatus_sie = const SSTATUS_SIE,
    );
}

