//! Process

use alloc::slice;
use alloc::boxed::Box;

use core::arch::naked_asm;

use crate::address::{align_up, PAddr, VAddr};
use crate::page::{map_page, PageTable, PAGE_SIZE, SATP_SV32, PAGE_R, PAGE_W, PAGE_X, PAGE_U};
use crate::scheduler::PROCS;
use crate::virtio::VIRTIO_BLK_PADDR;

unsafe extern "C" {
    // Safety: Symbols created by the linker script
    static __kernel_base: u8;
    static __free_ram_end: u8;
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum State {
    Unused,     // Unused process control structure
    Runnable,   // Runnable process
    Exited,     // Process exited
}

#[derive(Clone, Debug)]
pub struct Process {
    pub pid: usize,             // Process ID
    pub state: State,           // Process state
    pub sp: VAddr,              // Stack pointer
    pub page_table: Option<Box<PageTable>>,
    pub stack: [u8; 8192],      // Kernel stack
}

impl Process {
    pub const fn zeroed() -> Self {
        // Safety: All-zero bytes is a valid representation: integers become 0, pointer becomes null, is_kernel bool is false
        unsafe { core::mem::MaybeUninit::zeroed().assume_init() }
    }
}

// The base virtual address of an application image. This needs to match the
// starting address defined in `user.ld`.
const USER_BASE: usize = 0x1000000;
const SSTATUS_SUM: usize = 1 << 18;     // Supervisor read user pages

#[unsafe(naked)]
pub extern "C" fn user_entry() {
    naked_asm!("sret");
}

pub fn create_process(entry: usize, image: *const u8, image_size: usize) -> usize {
    let is_kernel = {image_size == 0 };         // Kernel processes have zero image size
    let mut procs = PROCS.0.lock();

    // Find an unused process control structure.
    let (i, process) = procs.iter_mut()
        .enumerate()
        .find(|(_, p)| p.state == State::Unused)
        .expect("no free process slots");

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
    };

    // Create CSRs for new process
    let page_table = process.page_table.as_ref().expect("page_table should exist");
    // Double deref on page_table for both ref and Box.
    let page_table_addr = &**page_table as *const PageTable as usize;
    let satp = SATP_SV32 | (page_table_addr / PAGE_SIZE);

    let (sscratch, sepc, sstatus) = if is_kernel {
        (0, 0, read_csr!("sstatus"))                // Kernel CSRs
    } else {                                        // User CSRs
        (process.stack.as_ptr_range().end as usize,
         USER_BASE,
         read_csr!("sstatus") | SSTATUS_SUM,
        )
    };

    // Stack callee-saved registers. These register values will be restored in
    // the first context switch in switch_context.
    let callee_saved_regs: [usize; 17] = [
        entry as usize, // ra
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
        sscratch,       // sscratch
        sepc,           // sepc
        sstatus,        // sstatus
        satp,           // satp
    ];

    // Place the callee-saved registers at the end of the stack
    let callee_saved_regs_start = process.stack.len() - callee_saved_regs.len() * size_of::<usize>();
    let mut offset = callee_saved_regs_start;
    for reg in &callee_saved_regs {
        let bytes = reg.to_ne_bytes(); // native endian
        process.stack[offset..offset + size_of::<usize>()].copy_from_slice(&bytes);
        offset += size_of::<usize>();
    }

    // Initialise fields.
    process.pid = i + 1;
    process.state = State::Runnable;
    process.sp = VAddr::new(&raw const process.stack[callee_saved_regs_start] as usize);

    process.pid
}
