//! Round-robin scheduler

use core::arch::asm;

use crate::allocator::PAGE_SIZE;
use crate::page::{SATP_SV32, PageTable};
use crate::process::{create_process, PROCS, PROCS_MAX, State, switch_context};
use crate::spinlock::SpinLock;

static FIRST_BOOT: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);
static mut DUMMY_SP: usize = 0;

static IDLE_PROC: SpinLock<Option<usize>> = SpinLock::new(None);    // Idle process
pub static CURRENT_PROC: SpinLock<Option<usize>> = SpinLock::new(None); // Currently running process
pub const IDLE_PID: usize = 0; // idle

fn idle_process() {
    panic!("reached idle process");
}

pub fn yield_now() {
    // Initialse IDLE_PROC if not yet initialised
    let idle_pid = { *IDLE_PROC.lock().get_or_insert_with(|| {
            let idle_pid = create_process(idle_process as *const() as usize, core::ptr::null(), 0);
            if let Some(p) = PROCS.0.lock().iter_mut()
                .find(|p| p.pid == idle_pid) {
                    p.pid = IDLE_PID;
                }
            *CURRENT_PROC.lock() = Some(IDLE_PID);
            IDLE_PID
        })
    };

    let current_pid = CURRENT_PROC.lock()
        .expect("CURRENT_PROC initialised before use");

    // Search for a runnable process
    let next_pid = PROCS.get_next(current_pid);

    // If there's no runnable process other than the current one, return and continue processing
    if next_pid == current_pid {
        return;
    }

    let (next_sp_ptr, current_sp_ptr, satp/*, sscratch*/) = {
        let next_index = PROCS.try_get_index(next_pid)
            .expect("should find next by pid");
        let current_index = PROCS.try_get_index(current_pid)
            .expect("should find current by pid");
        let mut procs = PROCS.0.lock();
        let [next, current] = procs.get_disjoint_mut([next_index, current_index])
            .expect("indices should be valid and distinct");

        let next_sp_ptr = next.sp.field_raw_ptr();

        let current_sp_ptr = if FIRST_BOOT.swap(false, core::sync::atomic::Ordering::Relaxed) {
            // First boot, create dummy sp pointer
            &raw mut DUMMY_SP
        } else {
            current.sp.field_raw_ptr()
        };

        let page_table = next.page_table.as_ref().expect("page_table should exist");
        // Double deref on page_table for both ref and Box.
        let page_table_addr = &**page_table as *const PageTable as usize;
        let satp = SATP_SV32 | (page_table_addr / PAGE_SIZE);
        //Safety: sscratch points to the end of next.stack, which is a valid stack allocation.

        // let sscratch = if next.is_kernel {
        //     0
        // } else {
        //     next.stack.as_ptr_range().end as usize
        // };
        // crate::println!("in scheduler, next sscratch is {sscratch:x}");

        (next_sp_ptr, current_sp_ptr, satp/*, sscratch*/)
    };

    // unsafe{asm!(
    //     "sfence.vma",
    //     "csrw satp, {satp}",
    //     "sfence.vma",
    //     // "csrw sscratch, {sscratch}",
    //     satp = in(reg) satp,
    //     // sscratch = in(reg) sscratch,
    // )};

    // Context switch
    *CURRENT_PROC.lock() = Some(next_pid);
    let interrupts_enabled: bool = (read_csr!("sstatus") & 0x2) != 0;

    // crate::println!("{PROCS}");

    // crate::println!("in yield_now just before context_switch!\n current_sp_ptr has address {:x}\n next_sp_ptr has address {:x}\n satp is {:x}", current_sp_ptr as usize, next_sp_ptr as usize, satp);
    unsafe {
        switch_context(current_sp_ptr, next_sp_ptr, interrupts_enabled, satp/*, sscratch*/);
    }
    // crate::println!("\nswitch_context function has returned\n");
    // crate::println!("sscratch is {:x}", read_csr!("sscratch"));
    // crate::println!("satp is {:x}", read_csr!("satp"));
    // let stackpointer: usize;
    // unsafe{asm!("mv {}, sp", out(reg) stackpointer);}
    // crate::println!("sp is {:x}", stackpointer);
    //
    // crate::println!("yield_now now returning");

}
