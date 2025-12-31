//! Process

use alloc::slice;
use alloc::boxed::Box;

use core::arch::{asm, naked_asm};
use core::fmt;

use crate::address::{align_up, PAddr, VAddr};
use crate::allocator::PAGE_SIZE;
use crate::entry::TrapFrame;
use crate::page::{map_page, PageTable, PAGE_R, PAGE_W, PAGE_X, PAGE_U};
use crate::scheduler::{CURRENT_PROC, IDLE_PID};
use crate::spinlock::SpinLock;
use crate::virtio::VIRTIO_BLK_PADDR;

unsafe extern "C" {
    static __kernel_base: u8;
    static __free_ram_end: u8;
}

pub const PROCS_MAX: usize = 8;         // Maximum number of processes

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum State {
    Unused,     // Unused process control structure
    Runnable,   // Runnable process
    Exited,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct Process {
    pub pid: usize,             // Process ID
    pub state: State,           // Process state: Unused or Runnable
    pub is_kernel: bool,        // Kernel process
    pub sp: VAddr,              // Stack pointer
    pub page_table: Option<Box<PageTable>>,
    pub stack: [u8; 8192],      // Kernel stack
}

impl Process {
    const fn empty() -> Self {
        Self {
            pid: 0,
            state: State::Unused,
            is_kernel: false,
            sp: VAddr::new(0),
            page_table: None,
            stack: [0; 8192],
        }
    }
}

pub struct Procs(pub SpinLock<[Process; PROCS_MAX]>);

impl Procs {
    const fn new() -> Self {
        Self(
            SpinLock::new([const { Process::empty() }; PROCS_MAX])
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

    // pub fn try_get_frame(&self, pid: usize) -> &mut TrapFrame {
    //     let index = PROCS.try_get_index(pid)
    //         .expect("process {pid} should be in procs");
    //     let mut procs = PROCS.0.lock();
    //     let frame = &mut procs[index];
    //     frame
    // }
}

// Optional - but vital for debugging if you want to print the contents of PROCS.
impl fmt::Display for Procs {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let procs = PROCS.0.lock();
        for (i, process) in procs.iter().enumerate() {
            write!(f, "Addr: {:x?} ", &raw const *process as usize)?;
            writeln!(f, "PROC[{i}]")?;
            write!(f, "PID: {} ", process.pid)?;
            write!(f, "SP: {:x?} ", process.sp)?;
            writeln!(f, "STATE: {:?} ", process.state)?;
            writeln!(f, "IS_KERNEL: {:?} ", process.is_kernel)?;
            writeln!(f, "STACK:  ... {:x?}", &process.stack[process.stack.len()-128..process.stack.len()])? // Remember range top is _exclusive_ hence no panic
        }
        Ok(())
    }
}

pub static PROCS: Procs = Procs::new();  // All process control structures.

// The base virtual address of an application image. This needs to match the
// starting address defined in `user.ld`.
const USER_BASE: usize = 0x1000000;
const SSTATUS_SPIE: usize =  1 << 5;    // Enable user mode
const SSTATUS_SUM: usize = 1 << 18;
const SSTATUS_SPP: usize = 1 << 8;      // Supervisor previous priv. level (user = 0, supervisor = 1)
pub const SSTATUS_SIE: usize = 1 << 1;     //  Enable supervisor interrupts

pub fn user_entry() {
    unsafe{asm!(
        "csrw sepc, {sepc}",
        "csrw sstatus, {sstatus}",
        "sret",
        sepc = in(reg) USER_BASE,
        sstatus = in(reg) (SSTATUS_SPIE | SSTATUS_SUM),
    )}
}

pub fn walk_page_table(table1: &PageTable, vaddr: VAddr) -> Option<(PAddr, usize)> {
    let vpn1 = vaddr.vpn1();

    // Check if level 1 entry exists
    let pte1 = table1[vpn1];
    if pte1 & crate::page::PAGE_V == 0 {
        crate::println!("Level 1 PTE not valid for vpn1={}", vpn1);
        return None;
    }

    // Get level 0 table
    let table0 = unsafe {
        let table0_paddr = PAddr::from_ppn(pte1);
        &*(table0_paddr.as_ptr() as *const PageTable)
    };

    // Check level 0 entry
    let vpn0 = vaddr.vpn0();
    let pte0 = table0[vpn0];
    if pte0 & crate::page::PAGE_V == 0 {
        crate::println!("Level 0 PTE not valid for vpn0={}", vpn0);
        return None;
    }

    // Extract physical address and flags
    let paddr = PAddr::from_ppn(pte0);
    let flags = pte0 & 0xFF; // Lower 8 bits are flags

    Some((paddr, flags))
}


pub fn create_process(entry: usize, image: *const u8, image_size: usize) -> usize {
    let is_kernel = {image_size == 0 };         // Kernel processes have zero image size
    let mut procs = PROCS.0.lock();

    // Find an unused process control structure.
    let (i, process) = procs.iter_mut()
        .enumerate()
        .find(|(_, p)| p.state == State::Unused)
        .expect("no free process slots");

    // Stack callee-saved registers. These register values will be restored in
    // the first context switch in switch_context.
    let callee_saved_regs: [usize; 16   ] = [
        entry as usize,            // ra
        0,              // s0
        0,              // s1
        0,              // s2
        0,              // s3
        0,              // s4
        0,              // s5
        0,              // s6
        0,              // s7
        0,              // s8
        0,              // s9
        0,              // s10
        0,              // s11
        if is_kernel {0} else { process.stack.as_ptr_range().end as usize },         // sscratch
        0,              // sepc
        read_csr!("sstatus"),              // sstatus
    ];

    // crate::println!("pid {} has callee-saved-regs {:x?}", i+1, callee_saved_regs);
    // Place the callee-saved registers at the end of the stack
    let callee_saved_regs_start = process.stack.len() - callee_saved_regs.len() * size_of::<usize>();
    let mut offset = callee_saved_regs_start;
    for reg in &callee_saved_regs {
        let bytes = reg.to_ne_bytes(); // native endian
        process.stack[offset..offset + size_of::<usize>()].copy_from_slice(&bytes);
        offset += size_of::<usize>();
    }

    // Map kernel pages.
    let mut page_table = Box::new(PageTable::new());
    let kernel_base = &raw const __kernel_base as usize;
    let free_ram_end = &raw const __free_ram_end as usize;

    for paddr in (kernel_base..free_ram_end).step_by(PAGE_SIZE) {
        map_page(page_table.as_mut(), VAddr::new(paddr), PAddr::new(paddr), PAGE_R | PAGE_W | PAGE_X);
    }

    map_page(page_table.as_mut(), VAddr::new(VIRTIO_BLK_PADDR as usize), PAddr::new(VIRTIO_BLK_PADDR as usize), PAGE_R | PAGE_W);

    process.page_table = Some(page_table);

    if !is_kernel {
        // Map user pages.
        let aligned_size = align_up(image_size, PAGE_SIZE);
        let image_slice = unsafe {
            slice::from_raw_parts(image, image_size)
        };
        let mut image_vec = image_slice.to_vec();
        image_vec.resize(aligned_size, 0);
        let image_data = Box::leak(image_vec.into_boxed_slice());
        let page_table = process.page_table.as_mut()
        .expect("page table must be initialized before mapping user pages");

        for (i, page_chunk) in image_data.chunks_mut(PAGE_SIZE).enumerate() {
            let vaddr = VAddr::new(USER_BASE + i * PAGE_SIZE);
            let paddr = PAddr::new(page_chunk.as_mut_ptr() as usize);

            map_page(
                page_table,
                vaddr,
                paddr,
                PAGE_U | PAGE_R | PAGE_W | PAGE_X,
            );
        }

        let fault_vaddr = VAddr::new(0x100085e);
        if let Some((paddr, flags)) = walk_page_table(page_table, fault_vaddr) {
            crate::println!("Fault addr 0x100085e -> paddr 0x{:x}, flags 0x{:x}",
                            paddr.as_usize(), flags);
            crate::println!("  V={} R={} W={} X={} U={}",
                            flags & crate::page::PAGE_V != 0,
                            flags & PAGE_R != 0,
                            flags & PAGE_W != 0,
                            flags & PAGE_X != 0,
                            flags & PAGE_U != 0);
        } else {
            crate::println!("ERROR: 0x100085e not mapped!");
        }
    };

    // Initialise fields.
    process.pid = i + 1;
    process.state = State::Runnable;
    process.is_kernel = is_kernel;
    process.sp = VAddr::new(&raw const process.stack[callee_saved_regs_start] as usize);

    process.pid
}

#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(
        prev_sp: *mut usize,        // a0
        next_sp: *mut usize,        // a1
        interrupts_enabled: bool,   // a2
        satp: usize,                // a3
        /*sscratch: usize*/) {          // a4
    naked_asm!(
        ".align 2",

        // Save current interrupt state and disable interrupts
        // "csrr t0, sstatus",         // Read sstatus
        // "andi t1, t0, 2",           // Extract SIE bit (bit 1)
        "csrci sstatus, 2",     // Disable interrupts

        // Save callee-saved registers onto the current process's stack.
        "addi sp, sp, -16 * 4", // Allocate stack space for 16 4-byte registers
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
        "csrr s0, sscratch",        // s0 is already stored, use as temp register to get current sscratch
        "sw s0, 13 * 4(sp)",        // Store sscratch of current process
        "csrr s0, sepc",
        "sw s0, 14 * 4(sp)",
        "csrr s0, sstatus",
        "sw s0, 15 * 4(sp)",

        // Switch the stack pointer.
        "sw sp, (a0)",              // *prev_sp = sp;
        "lw sp, (a1)",              // Switch stack pointer (sp) here

        // Switch satp
        "sfence.vma",
        "csrw satp, a3",
        "sfence.vma",
        // Restore callee-saved registers from the next process's stack.
        "lw s0, 13 * 4(sp)",
        "csrw sscratch, s0",        // Restore sscratch for next process
        "lw s0, 14 * 4(sp)",
        "csrw sepc, s0",
        "lw s0, 15 * 4(sp)",
        "csrw sstatus, s0",

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
        "addi sp, sp, 16 * 4",      // We've popped 14 4-byte registers from the stack
        "beqz a2, 1f",              // a2 = interrupts enabled is 0 (false)
        "csrsi sstatus, 2",         // Reenable interrupts

        "1:",
        "ret",
    );
}


// #[unsafe(naked)]
// pub unsafe extern "C" fn switch_context(prev_sp: *mut usize, next_sp: *mut usize) {
//     naked_asm!(
//         ".align 2",
//
//         // Save current interrupt state and disable interrupts
//         "csrr t0, sstatus",         // Read sstatus
//         "andi t1, t0, 2",           // Extract SIE bit (bit 1)
//     "csrci sstatus, 2",         // Disable interrupts
//
//     // Allocate stack space (13 registers + saved SIE state)
//     "addi sp, sp, -14 * 4",
//
//     // Save callee-saved registers
//     "sw ra,  0  * 4(sp)",
//     "sw s0,  1  * 4(sp)",
//     "sw s1,  2  * 4(sp)",
//     "sw s2,  3  * 4(sp)",
//     "sw s3,  4  * 4(sp)",
//     "sw s4,  5  * 4(sp)",
//     "sw s5,  6  * 4(sp)",
//     "sw s6,  7  * 4(sp)",
//     "sw s7,  8  * 4(sp)",
//     "sw s8,  9  * 4(sp)",
//     "sw s9,  10 * 4(sp)",
//     "sw s10, 11 * 4(sp)",
//     "sw s11, 12 * 4(sp)",
//     "sw t1,  13 * 4(sp)",       // Save SIE state
//
//     // Switch stack pointer
//     "sw sp, (a0)",
//     "lw sp, (a1)",
//
//     // Restore callee-saved registers
//     "lw ra,  0  * 4(sp)",
//     "lw s0,  1  * 4(sp)",
//     "lw s1,  2  * 4(sp)",
//     "lw s2,  3  * 4(sp)",
//     "lw s3,  4  * 4(sp)",
//     "lw s4,  5  * 4(sp)",
//     "lw s5,  6  * 4(sp)",
//     "lw s6,  7  * 4(sp)",
//     "lw s7,  8  * 4(sp)",
//     "lw s8,  9  * 4(sp)",
//     "lw s9,  10 * 4(sp)",
//     "lw s10, 11 * 4(sp)",
//     "lw s11, 12 * 4(sp)",
//     "lw t1,  13 * 4(sp)",       // Restore SIE state
//
//     "addi sp, sp, 14 * 4",
//
//     // Restore interrupt state (only if it was enabled before)
//     "beqz t1, 1f",              // If SIE was 0, skip re-enabling
//     "csrsi sstatus, 2",         // Re-enable interrupts
//     "1:",
//
//     "ret",
//     )
// }
