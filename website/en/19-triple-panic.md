# Triple Panic!

We are going to panic frequently as we build our operating system, and we need to be more robust in case we panic while we're panicing.

We do this with a global atomic which counts how many times we have panicked, and each time we try to get some message to the console.

# Atomic panic counter

First let's add the counter starting at zero to `panic.rs`:

```rust [kernel/src/panic.rs]

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
