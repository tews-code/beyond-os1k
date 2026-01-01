//! Timers

use core::arch::asm;

pub struct Timer;

impl Timer {
    pub fn set(&self, millisecs: u64) {
        let ticks = millisecs_to_ticks(millisecs);
        let current_ticks = get_timer();
        crate::sbi::set_timer(current_ticks + ticks)
        .expect("could not set timer");
    }
}

pub static TIMER: Timer = Timer;

fn millisecs_to_ticks(millisecs: u64) -> u64 {
    const FREQ: u64 = 10_000_000; // QEMU counter runs at 10 MHz ticks / second
    millisecs * FREQ / 1_000
}

#[inline]
fn get_timer() -> u64 {
    let mut ticksl: u32;
    let mut ticksh: u32;
    let mut ticksh_check: u32;
    loop { // Loop in case we read the low 32 bits of the counter just before overflow
        unsafe {
            asm!("rdtimeh {}", out(reg) ticksh, options(nomem, nostack, preserves_flags));
            asm!("rdtime {}", out(reg) ticksl, options(nomem, nostack, preserves_flags));
            asm!("rdtimeh {}", out(reg) ticksh_check, options(nomem, nostack, preserves_flags));
        }
        if ticksh_check == ticksh {
            break; // Did not overflow, leave the loop
        }
    }
    ((ticksh as u64) << 32) | (ticksl as u64)
}
