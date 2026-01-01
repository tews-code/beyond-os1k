# Let's Test!

So far we have used Cargo to manage our packages and build our code, but we've not taken advantage of the unit testing built into Cargo. Let's add this now, so that we can more confidently refactor our code as we add more features.

Testing with Cargo should be as easy as

```bash
$ cargo test
```
but once again `no_std` means we can't use the standard test crate that provides the features we need.

# Rust unstable "nightly"

Instead, we'll use a custom test feature. That means that we need to move away from using stable Rust, and instead use the "nightly" version which comes with many more features.

In your workspace root directory, create a new config file `rust-toolchain.toml` to set the Rust channel for this workspace:

```toml [rust-toolchain.toml]
[toolchain]
channel = "nightly"
```
You will need to update the toolchain using `rustup` and install the RISCV target as well:

```bash
$ rustup target install riscv32imac-unknown-none-elf
```

# Adding a custom test runner

Now that we are using "nightly", we can use a custom test runner that allows us to create tests in a `no_std` environment. 

> [!TIP]
> This is taken from the excellent blog at (https://os.phil-opp.com/testing/)[Writing an OS in Rust] by Philipp Oppermann. 

We start by adding some custom code in `kernel/src/main.rs`:

```rust [kernel/src/main.rs] {6-9}
//! OS in 1000 lines

#![no_std]
#![no_main]

#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(crate::test_runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]
...
```
Each of these statements starts with the `#![cfg_attr(test, ...)] compiler directive, which means that the directive only takes place if we run Cargo with "test". 

Each of these directives helps set up the custom test framework:
- `feature(custom_test_frameworks)` - Allows any function, const or static to be annotated with `#[test_case]` and it will be included in testing (https://doc.rust-lang.org/unstable-book/language-features/custom-test-frameworks.html)[The Rust Unstable Book].
- `test_runner(crate::test_runner)` - Sets the name of the function that aggregates and runs the tests - in this case to `fn test_runner`.
- `reexport_test_harness_main = "test_main")` - By default, the test framework will replace the existing `main` function, and use that to execute the tests. However, we don't have a `main` function, so we will instead re-export the testing function as `test_main`, and explicitly call `test_main` from our existing code when our operating system is ready.

Now let's add the test runner function. We'll only need this if we are testing, so we'll use a directive:

```rust [kernel/src/main.rs]

...

#[cfg(test)]
pub fn test_runner(tests: &[&dyn Fn()]) {
    println!("Running {} tests", tests.len());
    for test in tests {
        test();
    }
}
```

The custom test framework gathers functions annotated with `#[test_case]` into a slice, which is passed to `test_runner`. After printing the number of tests to be run, the slice is iterated and each test is run in turn.

> [!TIP]
> The function argument `tests` is a slice of references to `dyn Fn()`. These are each `dyn` functions - dynamic dispatch (i.e. the function is not known at compile time, and is instead determined at run time) and have the trait of `Fn()`, which is any function or closure that can be called multiple times and borrows captured variables immutably (https://doc.rust-lang.org/book/ch13-01-closures.html?highlight=FnMut#moving-captured-values-out-of-closures-and-the-fn-traits) [The Rust Programming Language]. In our case, our test functions don't capture any environment variables so will have the `Fn()` trait.

# Call `test_main` when ready

In our `no_std` environment we need to set up enough of our operating system before we can begin testing. Let's set this up after we have added the kernel entry `stvec` and initialised virtio and fs within `kernel_main`:

```rust [kernel/src/main.rs] {17-18}
...

#[unsafe(no_mangle)]
fn kernel_main() -> ! {
    let bss = &raw const __bss;
    let bss_end = &raw const __bss_end;
    // Safety: from linker script bss is aligned and bss segment is valid for writes up to bss_end
    unsafe {
        write_bytes(bss as *mut u8, 0, bss_end as usize - bss as usize);
    }

    write_csr!("stvec", kernel_entry as *const () as usize);

    virtio_blk_init();
    fs_init();

    #[cfg(test)]
    test_main();
    
    ...
```

# Add a trivial test

To check that this is working, let's add a base case.

```rust [kernel/src/main.rs]

...
#[cfg(test)]
mod test {
    use super::*;

    #[test_case]
    fn trivial_test() {
        // Trivial test to ensure test_runner is working
        // Deliberately doesn't print to avoid invoking code
        print!("trivial assertion... ");
        assert!(1 == 1);

        println!("[\x1b[32mok\x1b[0m]");
    }
}
```
We only create the test module when the `test` attribute is being used. Within the module, we give ourselves use of everything in the `super` module, so that we can test any items in the parent module.

We annotate the test function, and within the test function we print a brief description, assert that 1 = 1, and then print `[ok]`. (`\x1b[32m` and `\x1b[0m]`) set the terminal font to green, if your terminal supports colour.

# Set the run path

Cargo has a surprise for us - while our kernel binary is run from the workspace root directory, when testing Cargo runs the tests in each _package_ root. This breaks our `run.sh` which expects to be running from the workspace root. 

To force this, we need to first ask Cargo to create an environment variable for us, and then use that in our run script. 

First, edit the config file at `.cargo/config.toml`:

```toml [.cargo/config.toml] {8-9}
[build]
target="riscv32imac-unknown-none-elf"
rustflags = ["-g", "-O"]

[target.riscv32imac-unknown-none-elf]
runner = "./run.sh"

[env]
CARGO_WORKSPACE_DIR = { value = "", relative = true }
```
This creates a new environment variable called `CARGO_WORKSPACE_DIR` which has the path relative to the config file (and an empty value which would otherwise add a subdirectory).


In `run.sh`, we can force our runner to change to the workspace root before running QEMU:

```bash [run.sh]

# Run this relative to the workspace directory
cd "$CARGO_WORKSPACE_DIR"

```

If you use `rust-analyzer` then you may also want to edit `kernel/Cargo.toml` and `user/Cargo.toml` to allow for testing:

```toml [kernel/Cargo.toml] {5}

[[bin]]
name = "kernel"
test = true
doctest = false
bench = false

```

Now that we have our configuration in place, we can proceed.


# Run the test

Now we are finally ready to run the test. First, make sure that the shell binary has been compiled and transformed. Then run the test.

```bash
$ ./os1k.sh build
$ cargo test

virtio-blk: capacity is 10240 bytes
........................file: hello.txt, size=83
file: meow.txt, size=6
Running 1 tests
trivial assertion... [ok]
```

Our test passed! We can now add unit tests to each of our modules, and catch mistakes much earlier in our development.

It's worth noting that our tests are simple - if they pass we print "ok", but if they fail we panic. This does mean that (unlike the standard Rust test crate) we can't deliberately test for panics. We also can't test printed output, as there is no way to capture that output within the test framework. 

# Testing within the shell

If we want to test other packages in our workspace, we need to make the same changes as we did for our kernel. In the case of the shell, that means adding to `user/src/bin/shell.rs` as this has the ability to call `test_main` in user space.

```rust [user/src/bin/shell.rs]

//! os1k shell

#![no_std]
#![no_main]

#![cfg_attr(test, feature(custom_test_frameworks))]
#![cfg_attr(test, test_runner(crate::test_runner))]
#![cfg_attr(test, reexport_test_harness_main = "test_main")]

...

#[unsafe(no_mangle)]
fn main() {

    #[cfg(test)]
    test_main();

    loop {
    
    ...
    
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{print, println};

    #[test_case]
    fn shell_trivial_test() {
        print!("shell: trivial test...");

        assert!(1 == 1);

        println!("[\x1b[32mok\x1b[0m]");
    }
}

#[cfg(test)]
pub fn test_runner(tests: &[&dyn Fn()]) {
    println!("Running {} user tests", tests.len());
    for test in tests {
        test();
    }
}
```
To run the tests in user space, we need a way to build the shell with the test attribute. Let's add this to our build script `os1k.sh`:

```bash [osk1.sh]

...

if [ "$COMMAND" == "test" ]; then
    cargo clean;
    TEST_BINARY=$(cargo test --no-run -p user --bin shell --message-format=json 2>/dev/null | \
    sed -n 's/.*"executable":"\([^"]*\)".*/\1/p' | \
    head -n 1);
    cd $TARGET_DIR;
    cp "$TEST_BINARY" shell

    $OBJCOPY --set-section-flags=.bss=alloc,contents \
        --output-target=binary \
        shell shell.bin;
    cp shell.bin "$CWD";
    $OBJCOPY -Ibinary -Oelf32-littleriscv shell.bin shell.bin.o;
    file shell.bin.o;
    cp shell.bin.o "$CWD";
    cd "$CWD";
    cargo test --bin kernel;
fi

...
    
```
The command `cargo test --no-run -p user --bin shell --message-format=json` creates a test shell binary without running it, and also asks Cargo to give all the compilation details in JSON format. The clever `sed` script then finds the name of the shell binary (which is something like `shell-db397d7cd59a3df3`) and we copy this to our familiar name `shell` before performing objcopy.

Let's try testing kernel and shell together!

```bash

$ ./os1k.sh test

virtio-blk: capacity is 10240 bytes
file: hello.txt, size=83
file: meow.txt, size=6
Running 1 tests
trivial assertion... [ok]
Hello World! 🦀
Running 1 user tests
shell: trivial test...[ok]
```

Success! Going forward, we can add tests as we create or refactor our kernel modules, using Cargo's custom test framework feature!
