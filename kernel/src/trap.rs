//! Trap handler

use alloc::slice;

use common::{
    SYS_PUTBYTE,
    SYS_GETCHAR,
    SYS_EXIT,
    SYS_READFILE,
    SYS_WRITEFILE,
};

use crate::process::State;
use crate::sbi::{put_byte, get_char};
use crate::scheduler::{yield_now, PROCS, CURRENT_PROC, SSTATUS_SIE};
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

#[unsafe(no_mangle)]
pub extern "C" fn handle_trap(f: &mut TrapFrame) {
    let scause = read_csr!("scause");
    if scause == SCAUSE_ECALL {
        let mut user_pc = read_csr!("sepc");
        write_csr!("sstatus", read_csr!("sstatus") | SSTATUS_SIE);  // Re-enable interrupts
        handle_syscall(f);
        user_pc += 4;
        write_csr!("sepc", user_pc);
    } else if scause == SCAUSE_TIMER_INTERRUPT {
        TIMER.set(500);
        write_csr!("sstatus", read_csr!("sstatus") | SSTATUS_SIE);  // Re-enable interrupts
        yield_now();
    } else {
        panic!("unexpected trap scause=0x{:x}, stval=0x{:x}, sepc=0x{:x}", scause, read_csr!("stval"), read_csr!("sepc"));
    }
}

fn handle_syscall(f: &mut TrapFrame) {
    let sysno = f.a7;
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
        SYS_READFILE | SYS_WRITEFILE => 'readorwritefile: {
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

            let Some(file_i) = FILES.fs_lookup(filename) else {
                println!("file not found {:x?}", filename);
                f.a0 = usize::MAX; // 2's complement is -1
                break 'readorwritefile;
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
