use log::trace;
use sysinfo::System;

pub mod cpu;
pub mod ip;
pub mod load;
pub mod mem;
pub mod network;
pub mod os;

pub fn realtime_uptime() -> u64 {
    let uptime = System::uptime();
    trace!("REALTIME UPTIME successfully retrieved: {uptime}");
    uptime
}

pub fn realtime_process(sysinfo_sys: &System) -> u64 {
    // Use sysinfo process list for cross-platform support instead of reading /proc
    let process_count = u64::try_from(sysinfo_sys.processes().len()).unwrap_or(0);
    trace!("REALTIME PROCESS successfully retrieved: {process_count}");
    process_count
}
