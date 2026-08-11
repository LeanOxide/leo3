//! Example demonstrating IO monad support in leo3.
//!
//! This example shows how to use Lean4's IO primitives from Rust,
//! including console I/O, file handle operations, and IO sequencing.

use leo3::io::handle::FileMode;
use leo3::io::{console, handle};
use leo3::prelude::*;

fn main() -> LeanResult<()> {
    // Initialize Lean runtime
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        println!("=== IO Monad Example ===\n");

        // 1. Console I/O
        println!("1. Console I/O:");
        let io = console::put_str_ln(lean, "Hello from Lean IO!")?;
        io.run()?;

        // 2. Pure values in IO monad
        println!("\n2. Pure values:");
        let pure_io = leo3::io::LeanIO::pure(lean, 42)?;
        let value = pure_io.run()?;
        println!("Pure value: {}", value);

        // 3. Using map to transform IO values
        println!("\n3. Using map to transform IO values:");
        let io = leo3::io::LeanIO::pure(lean, 21)?;
        let doubled = io.map(lean, |x| x * 2)?;
        let result = doubled.run()?;
        println!("21 * 2 = {}", result);

        // Chain multiple maps
        let io = leo3::io::LeanIO::pure(lean, 10)?;
        let result = io
            .map(lean, |x| x + 5)?
            .map(lean, |x| x * 2)?
            .map(lean, |x| format!("Result: {}", x))?
            .run()?;
        println!("Chained maps: {}", result);

        // 4. Using bind to chain IO computations
        println!("\n4. Using bind to chain IO computations:");
        let io1 = leo3::io::LeanIO::pure(lean, 21)?;
        let io2 = io1.bind(lean, |x| {
            leo3::io::LeanIO::pure(lean, format!("The answer is {}", x * 2))
        })?;
        let result = io2.run()?;
        println!("{}", result);

        // Chain multiple binds
        let io = leo3::io::LeanIO::pure(lean, 5)?;
        let result = io
            .bind(lean, |x| leo3::io::LeanIO::pure(lean, x + 10))?
            .bind(lean, |x| leo3::io::LeanIO::pure(lean, x * 3))?
            .bind(lean, |x| {
                leo3::io::LeanIO::pure(lean, format!("Final: {}", x))
            })?
            .run()?;
        println!("Chained binds: {}", result);

        // 5. Sequencing IO operations
        println!("\n5. Sequencing IO operations:");
        let io1 = console::put_str(lean, "First... ")?;
        let io2 = console::put_str_ln(lean, "Second!")?;
        let sequenced = io1.then(io2);
        sequenced.run()?;

        // 6. File handle operations
        println!("\n6. File handle operations:");

        // Write to a file using handles
        let write_handle_io = handle::open(lean, "/tmp/leo3_test.txt", FileMode::Write)?;
        let write_handle = write_handle_io.run()?;

        handle::write(lean, &write_handle, "Hello from leo3!\n")?.run()?;
        handle::write(lean, &write_handle, "This is line 2.\n")?.run()?;
        handle::flush(lean, &write_handle)?.run()?;
        handle::close(lean, write_handle)?.run()?;

        println!("Wrote to /tmp/leo3_test.txt");

        // Read from the file using handles
        let read_handle_io = handle::open(lean, "/tmp/leo3_test.txt", FileMode::Read)?;
        let read_handle = read_handle_io.run()?;

        let line1_io = handle::get_line(lean, &read_handle)?;
        let line1 = line1_io.run()?;
        println!("Read line 1: {}", line1.trim());

        let line2_io = handle::get_line(lean, &read_handle)?;
        let line2 = line2_io.run()?;
        println!("Read line 2: {}", line2.trim());

        let eof_io = handle::is_eof(lean, &read_handle)?;
        let at_eof = eof_io.run()?;
        println!("At EOF: {}", at_eof);

        handle::close(lean, read_handle)?.run()?;

        // 7. Standard streams
        println!("\n7. Standard streams:");
        leo3::io::console::put_str(lean, "Writing to stdout via console\n")?.run()?;

        println!("\n=== All IO operations completed successfully! ===");

        Ok(())
    })
}
