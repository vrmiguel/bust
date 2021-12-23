use clap::Parser;
use uname::Uname;

use crate::{memory::lock_memory_pages, monitor::Monitor};

mod cfg;
mod daemon;
mod errno;
mod error;
mod kill;
mod linux_version;
mod memory;
mod monitor;
mod process;
mod uname;
mod utils;

fn main() -> error::Result<()> {
    let args = cfg::Cli::parse();
    println!("{:?}", args);

    // Show uname info and return the Linux version running
    let _linux_version = {
        let uname = Uname::new()?;
        let _ = uname.print_info();
        // TODO: check if Linux version is updated enough
        uname.parse_version().ok()
    };

    // In order to correctly use `mlockall`, we'll try our best to avoid heap allocations and
    // reuse these buffers right here, even though it makes the code less readable.
    // Buffer specific to process creation
    let proc_buf = [0_u8; 50];

    // Buffer for anything else
    let buf = [0_u8; 100];

    if !args.no_daemonize {
        // Daemonize current process
        println!("\nStarting daemonization process!");
        daemon::daemonize()?;
    }

    // Attempt to lock the memory pages mapped to the daemon
    // in order to avoid being sent to swap when the system
    // memory is stressed
    if let Err(err) = lock_memory_pages() {
        eprintln!("Failed to lock memory pages: {:?}. Continuing anyway.", err);
    } else {
        // Save this on both bustd.out and bustd.err
        println!("Memory pages locked!");
        eprintln!("Memory pages locked!");
    }

    Monitor::new(proc_buf, buf, args)?.poll()
}
