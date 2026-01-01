# User Documentation

Let's take some time to use the Rust documentation functions. We'll document the user-facing functions and deliberately hide kernel functions from the documentation.

# Rust documentation

Rust comes with a built-in Markdown implementation. Adding the right comments in front of modules, functions or items will allow `rustdoc` to produce a website with documentation for our kernel. Since we are using Cargo throughout our project, we'll use `cargo doc` to make the necssary calls for us.

But we don't want to document internal features of our kernel - rather we want to document the system calls and the shell, so that we can confidently build new applications in the future. 

First let's tell Cargo not to produce documentation for the `kernel` and `common` packages, by editing their respective `Cargo.toml` files and adding the following:

```toml [kernel/Cargo.toml] {12}
[package]
name = "kernel"
version = "0.1.0"
edition = "2024"
default-run = "kernel"

[[bin]]
name = "kernel"
test = false
doctest = false
bench = false
doc = false

[dependencies]
common = { workspace = true }
```

We can also hide individual functions from documentation by annotating them with 

```rust
#[doc(hidden)]
```
and we will use this for some of the core functions in our user library that are not intended for wider use.

It's worth noting that there are options for Rust to produce documentation for all functions in a package, so nothing is ever truly hidden.

# Add documentation

Rust encourages you to keep documentation up to date with your code. There are two special types of comments which we can use to document the code and which will appear in documentation. 

The first is module level comments, using `//!` and appearing at the top of a module. We have been using this add a one-line comment for our modules up to now. Let's extend the comment at the top of our shell module to give the user more detail:

```rust [user/src/bin/shell.rs]
//! os1k shell
//!
//! Very simple shell supporting these commands:
//! - `hello` - Prints a welcome message
//! - `readfile` - Reads the first 128 bytes of the file "hello.txt" and prints these to the debug console
//! - `writefile` - Writes the text "Hello from the shell!" to the file "meow.txt"
//! - `exit` - Exits the shell
```

At the same time, let's make sure the `main` function (which is not meant for external use) is hidden from the documentation:

```rust [user/src/bin/shell.rs] {3}
...
#[unsafe(no_mangle)]
#[doc(hidden)]
fn main() {
```

# User library documentation

The second type of comment that appears in documentation is `///`. Let's use this to document the user library:

```rust [user/src/lib.rs]
//! User library for os1k
//!
//! System calls for os1k user processes.

#![no_std]

use core::arch::{asm, naked_asm};
use core::panic::PanicInfo;

pub use common::{print, println};

use common::{
    SYS_PUTBYTE,
    SYS_GETCHAR,
    SYS_EXIT,
    SYS_READFILE,
    SYS_WRITEFILE,
};

/// User panic handler
///
/// Prints a panic message and exits the process.
#[panic_handler]
pub fn panic(info: &PanicInfo) -> ! {
    println!("😬 User Panic! {}", info);
    exit();
}

unsafe extern "C" {
    static __user_stack_top: u8;
}


#[doc(hidden)]
pub fn sys_call(sysno: usize, arg0: isize, arg1: isize, arg2: isize, arg3: isize) -> isize {
    let a0: isize;
    unsafe{asm!(
        "ecall",
        inout("a0") arg0 => a0,
        in("a1") arg1,
        in("a2") arg2,
        in("a3") arg3,
        in("a4") sysno,
    )}
    a0
}

/// Put a byte onto the debug console
///
/// Returns `Err` if the function fails.
/// Must be called repeatedly for each byte of a multibyte character.
#[unsafe(no_mangle)]
pub fn put_byte(b: u8) -> Result<(), isize> {
    let result = sys_call(SYS_PUTBYTE, b as isize, 0, 0, 0);
    if result == 0 {
        Ok(())
    } else {
        Err(result)
    }
}

/// Get character (or more accurately a byte) from the debug console
///
/// If no character is read, returns `None`.
///
/// Characters are returned as `usize` values. For multibyte characters, the function must be called for each byte.
///
/// Does not block.
pub fn get_char() -> Option<usize> {
    let ch = sys_call(SYS_GETCHAR, 0, 0, 0, 0);
    if ch == -1 {
        None
    } else {
        Some(ch as usize)
    }
}


/// Exit the process
///
/// System call to exit the process immediately.
#[unsafe(no_mangle)]
pub fn exit() -> ! {
    let _ = sys_call(SYS_EXIT, 0, 0, 0, 0);
    unreachable!("just in case!");
}

/// Read a text file from the file system
///
/// - `filename`: Complete file name as a Rust string slice
/// - `buf`: Byte buffer to receive the file contents
pub fn readfile(filename: &str, buf: &mut [u8]) {
    let _ = sys_call(SYS_READFILE, filename.as_ptr() as isize, filename.len() as isize, buf.as_mut_ptr() as isize, buf.len() as isize);
}

/// Write text to file
///
/// - `filename`: Complete file name as a Rust string slice
/// - `buf`: Byte buffer which will be written to the file
pub fn writefile(filename: &str, buf: &[u8]) {
    let _ = sys_call(SYS_WRITEFILE, filename.as_ptr() as isize, filename.len() as isize,  buf.as_ptr() as isize, buf.len() as isize);
}

#[unsafe(link_section = ".text.start")]
#[unsafe(no_mangle)]
#[unsafe(naked)]
unsafe extern "C" fn start() {
    naked_asm!(
        "la sp, {stack_top}",
        "call main",
        "call exit",
        stack_top = sym __user_stack_top
    )
}
```

# Code comments

The third type of comment that Rust supports is the simple `//`. These are comments that are just for code, and should not appear in documentation.

> [!TIP]
> Rust also supports `doctest`, which allows examples in documentation to be run as tests. This is a very powerful way to make sure documentation and code stay in sync. Unfortunately, in our `no_std` environment we can't make use of these tests.

# Documenting as we go

Going forward, we'll make sure to document code intended for users, so that our operating system is well documented!

// Panic counter. Every time the kernel panics, this counter is incremented.
static PANIC_COUNTER: AtomicU8 = AtomicU8::new(0);

```
Now let's match on the number of panics. If it is our first panic, let's print a normal panic message.


```rust [kernel/src/panic.rs]
// Kernel panic handler.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // In case it panics while handling a panic, this panic handler implements
    // some fallback logic to try to at least print the panic details.
    match PANIC_COUNTER.fetch_add(1, SeqCst)
    {
        0 => {
            // First panic: Try whatever we can do including complicated stuff
            // which may panic again.
            println!("⚠️ Panic: {}", info);

            loop {
                unsafe{asm!("wfi", options(readonly, nostack, noreturn))}
            }
        },
        
        _ => {
            loop {};
        }
    }
}
```
The critical point is to use `fetch_add` with the strictest `SeqCst` memory ordering. `fetch_add` atomically fetches the variable's value and increments it.

In our first panic we simply print the panic info as normal.

But what if `println!` itself is broken, and causes a panic? That would call this function again, but this time our `PANIC_COUNTER` will have a value of `1`. 

Let's extend match to cover this scenario:

```rust [kernel/src/panic.rs]

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {

    match PANIC_COUNTER.fetch_add(1, SeqCst)
    {
        0 => { /* Omitted */
        },
        1 => {
            // Double panics: panicked while handling a panic. Keep it simple and avoid print macros.
            for b in "⚠️⚠️ Double panic: ".bytes() {
                let _ = crate::sbi::put_byte(b);
            }
            if let Some(s) = info.message().as_str() {
                for b in s.bytes() {
                    let _ = crate::sbi::put_byte(b);
                }
            } else {
                let _ = crate::sbi::put_byte(b'!');
            }

            loop {
                unsafe{asm!("wfi", options(readonly, nostack, noreturn))}
            }
        },
        _ => {
            // Triple panics: println! and put_byte seem to be broken. Spin forever.
            // 🫠 is [240, 159, 171, 160]
            unsafe{
                asm!("ecall", in("a0") 240u8, in("a7") 0x1);
                asm!("ecall", in("a0") 159u8, in("a7") 0x1);
                asm!("ecall", in("a0") 171u8, in("a7") 0x1);
                asm!("ecall", in("a0") 160u8, in("a7") 0x1);
            }

            loop {
                unsafe{asm!("wfi", options(readonly, nostack, noreturn))}
            }
        }
    }
}
```

That should cover us for most scenarios!
