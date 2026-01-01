//! Allocate memory pages

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::write_bytes;

use crate::address::{align_up, PAddr};
use crate::page::PAGE_SIZE;
use crate::spinlock::SpinLock;

//Safety: Symbols created by linker script
unsafe extern "C" {
    static __free_ram: u8;
    static __free_ram_end: u8;
}

#[derive(Debug)]
struct BumpAllocator(SpinLock<Option<PAddr>>);

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator(
    SpinLock::new(None),
);

unsafe impl GlobalAlloc for BumpAllocator {
    // Safety: Caller must ensure that Layout has a non-zero size
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        debug_assert!(layout.size() > 0, "allocation size must be non-zero");

        let mut next_paddr = self.0.lock();

        // Initialise on first use
        let mut paddr = *next_paddr.get_or_insert_with(|| {
            PAddr::new(&raw const __free_ram as usize)
        });

        let aligned_size = align_up(layout.size(), PAGE_SIZE);

        let new_paddr = paddr.as_usize() + aligned_size;
        if new_paddr > &raw const __free_ram_end as usize {
            panic!("out of memory");
        }

        *next_paddr = Some(PAddr::new(new_paddr));

        unsafe{
            // Safety: paddr.as_ptr_mut() is aligned and not null; entire aligned_size of bytes is available for write
            write_bytes(paddr.as_ptr_mut() as *mut u8, 0x55, aligned_size)
        };

        paddr.as_ptr() as *mut u8
    }

    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}

#[cfg(test)]
mod test {
    use alloc::vec;
    use crate::{print, println};

    #[test_case]
    fn allocate_a_vec() {
        print!("allocator: allocate a vec...");

        let v = vec![1, 2, 3];
        assert!(v == [1, 2, 3]);

        println!("[\x1b[32mok\x1b[0m]");
    }
}
