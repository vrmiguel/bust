use std::time::Duration;

use crate::cfg::Cli;
use crate::error::Result;
use crate::kill;
use crate::memory;
use crate::memory::MemoryInfo;
use crate::process::Process;

enum MemoryStatus {
    NearTerminal(f32),
    Okay,
}

pub struct Monitor {
    memory_info: MemoryInfo,
    proc_buf: [u8; 50],
    buf: [u8; 100],
    status: MemoryStatus,
    args: Cli,
}

impl Monitor {
    /// Determines how much oomf should sleep
    /// This function is essentially a copy of how earlyoom calculates its sleep time
    ///
    /// Credits: https://github.com/rfjakob/earlyoom/blob/dea92ae67997fcb1a0664489c13d49d09d472d40/main.c#L365
    /// MIT Licensed
    pub fn sleep_time_ms(&self) -> Duration {
        // Maximum expected memory fill rate as seen
        // with `stress -m 4 --vm-bytes 4G`
        const RAM_FILL_RATE: i64 = 6000;
        // Maximum expected swap fill rate as seen
        // with membomb on zRAM
        const SWAP_FILL_RATE: i64 = 800;

        // Maximum and minimum time to sleep, in ms.
        const MIN_SLEEP: i64 = 100;
        const MAX_SLEEP: i64 = 1000;

        // TODO: make these percentages configurable by args./config. file
        const RAM_TERMINAL_PERCENT: f64 = 10.;
        const SWAP_TERMINAL_PERCENT: f64 = 10.;

        let ram_headroom_kib = (self.memory_info.available_ram_percent as f64
            - RAM_TERMINAL_PERCENT)
            * (self.memory_info.total_ram_mb as f64 * 10.0);
        let swap_headroom_kib = (self.memory_info.available_swap_percent as f64
            - SWAP_TERMINAL_PERCENT)
            * (self.memory_info.total_swap_mb as f64 * 10.0);

        let ram_headroom_kib = i64::max(ram_headroom_kib as i64, 0);
        let swap_headroom_kib = i64::max(swap_headroom_kib as i64, 0);

        let time_to_sleep = ram_headroom_kib / RAM_FILL_RATE + swap_headroom_kib / SWAP_FILL_RATE;
        let time_to_sleep = i64::min(time_to_sleep, MAX_SLEEP);
        let time_to_sleep = i64::max(time_to_sleep, MIN_SLEEP);

        Duration::from_millis(time_to_sleep as u64)
    }

    pub fn new(proc_buf: [u8; 50], mut buf: [u8; 100], args: Cli) -> Result<Self> {
        let memory_info = MemoryInfo::new()?;
        let status = if memory_info.available_ram_percent <= 15 {
            MemoryStatus::NearTerminal(memory::pressure::pressure_some_avg10(&mut buf)?)
        } else {
            MemoryStatus::Okay
        };

        Ok(Self {
            memory_info,
            proc_buf,
            buf,
            status,
            args,
        })
    }

    fn memory_is_low(&self) -> bool {
        let terminal_psi = self.args.cutoff_psi;
        matches!(self.status, MemoryStatus::NearTerminal(psi) if psi >= terminal_psi)
    }

    fn get_victim(&mut self) -> Result<Process> {
        kill::choose_victim(&mut self.proc_buf, &mut self.buf, &self.args)
    }

    fn update_memory_stats(&mut self) -> Result<()> {
        self.memory_info = memory::MemoryInfo::new()?;
        self.status = if self.memory_info.available_ram_percent <= 15 {
            let psi = memory::pressure::pressure_some_avg10(&mut self.buf)?;
            MemoryStatus::NearTerminal(psi)
        } else {
            MemoryStatus::Okay
        };
        Ok(())
    }

    fn free_up_memory(&mut self) -> Result<()> {
        let victim = self.get_victim()?;

        // TODO: is this necessary?
        //
        // Check for memory stats again to see if the
        // low-memory situation was solved while
        // we were searching for our victim
        self.update_memory_stats()?;
        if self.memory_is_low() {
            if self.args.kill_pgroup {
                kill::kill_process_group(victim)?;
            } else {
                kill::kill_and_wait(victim)?;
            }
        }
        Ok(())
    }

    // Use the never type here whenever it reaches stable
    #[allow(unreachable_code)]
    pub fn poll(&mut self) -> Result<()> {
        loop {
            // Update our memory readings
            self.update_memory_stats()?;
            if self.memory_is_low() {
                self.free_up_memory()?;
            }

            // Calculating the adaptive sleep time
            let sleep_time = self.sleep_time_ms();
            if self.args.verbose {
                eprintln!("[adaptive-sleep] {}ms", sleep_time.as_millis());
            }

            std::thread::sleep(sleep_time);
        }
        Ok(())
    }
}
