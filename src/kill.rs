use std::fs;
use std::time::Duration;
use std::{ffi::OsStr, time::Instant};

use libc::kill;
use libc::{EINVAL, EPERM, ESRCH, SIGKILL, SIGTERM};

use crate::errno::errno;
use crate::error::{Error, Result};
use crate::process::Process;
use crate::{cfg, utils};

pub fn choose_victim(proc_buf: &mut [u8], buf: &mut [u8], args: &cfg::Cli) -> Result<Process> {
    let now = Instant::now();

    // `args` is currently only used when checking for unkillable patterns
    #[cfg(not(feature = "glob-ignore"))]
    let _ = args;

    let mut processes = fs::read_dir("/proc/")?
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            entry
                .path()
                .file_name()
                .unwrap_or_else(|| OsStr::new("0"))
                .to_str()
                .unwrap_or("0")
                .trim()
                .parse::<u32>()
                .ok()
        })
        .filter(|pid| *pid > 1)
        .filter_map(|pid| Process::from_pid(pid, proc_buf).ok());

    let first_process = processes.next();
    if first_process.is_none() {
        // Likely an impossible scenario but we found no process to kill!
        return Err(Error::ProcessNotFound("choose_victim"));
    }

    let mut victim = first_process.unwrap();
    // TODO: find another victim if victim.vm_rss_kib() fails here
    let mut victim_vm_rss_kib = victim.vm_rss_kib(buf)?;

    for process in processes {
        if victim.oom_score > process.oom_score {
            // Our current victim is less innocent than the process being analysed
            continue;
        }

        #[cfg(feature = "glob-ignore")]
        {
            if let Some(patterns) = &args.ignored {
                if matches!(process.is_unkillable(buf, patterns), Ok(true)) {
                    continue;
                }
            }
        }

        let cur_vm_rss_kib = process.vm_rss_kib(buf)?;
        if cur_vm_rss_kib == 0 {
            // Current process is a kernel thread
            continue;
        }

        if process.oom_score == victim.oom_score && cur_vm_rss_kib <= victim_vm_rss_kib {
            continue;
        }

        let cur_oom_score_adj = match process.oom_score_adj(buf) {
            Ok(oom_score_adj) => oom_score_adj,
            // TODO: warn that this error happened
            Err(_) => continue,
        };

        if cur_oom_score_adj == -1000 {
            // Follow the behaviour of the standard OOM killer: don't kill processes with oom_score_adj equals to -1000
            continue;
        }

        // eprintln!("[DBG] New victim with PID={}!", process.pid);
        victim = process;
        victim_vm_rss_kib = cur_vm_rss_kib;
    }

    println!("[LOG] Found victim in {} secs.", now.elapsed().as_secs());
    println!(
        "[LOG] Victim => pid: {}, comm: {}, oom_score: {}",
        victim.pid,
        victim.comm(buf).unwrap_or("unknown").trim(),
        victim.oom_score
    );

    Ok(victim)
}

pub fn kill_process(pid: i32, signal: i32) -> Result<()> {
    let res = unsafe { kill(pid, signal) };

    if res == -1 {
        return Err(match errno() {
            // An invalid signal was specified
            EINVAL => Error::InvalidSignal,
            // Calling process doesn't have permission to send signals to any
            // of the target processes
            EPERM => Error::NoPermission,
            // The target process or process group does not exist.
            ESRCH => Error::ProcessNotFound("kill"),
            _ => Error::UnknownKill,
        });
    }

    Ok(())
}

pub fn kill_process_group(process: Process) -> Result<()> {
    let pid = process.pid;

    let pgid = utils::get_process_group(pid as i32)?;

    // TODO: kill and wait
    let _ = kill_process(-pgid, SIGTERM);

    Ok(())
}

/// Tries to kill a process and wait for it to exit
/// Will first send the victim a SIGTERM and escalate to SIGKILL if necessary
/// Returns Ok(true) if the victim was successfully terminated
pub fn kill_and_wait(process: Process) -> Result<bool> {
    let pid = process.pid;
    let now = Instant::now();

    let _ = kill_process(pid as i32, SIGTERM);

    let half_a_sec = Duration::from_secs_f32(0.5);
    let mut sigkill_sent = false;

    for _ in 0..20 {
        std::thread::sleep(half_a_sec);
        if !process.is_alive() {
            println!("[LOG] Process with PID {} has exited.\n", pid);
            return Ok(true);
        }
        if !sigkill_sent {
            let _ = kill_process(pid as i32, SIGKILL);
            sigkill_sent = true;
            println!(
                "[LOG] Escalated to SIGKILL after {} nanosecs",
                now.elapsed().as_nanos()
            );
        }
    }

    Ok(false)
}
