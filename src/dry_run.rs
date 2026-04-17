use crate::get_info::cpu::cpu_info_without_usage;
use crate::get_info::load::realtime_load;
use crate::get_info::mem::{filter_disks, mem_info_without_usage, realtime_mem, realtime_swap};
use crate::get_info::network::should_filter_interface;
use crate::get_info::network::realtime_connections;
use log::info;
use sysinfo::{Disks, Networks};

pub async fn dry_run() {
    info!("The following is the equipment that will be put into operation and monitored:");
    let mut sysinfo_sys = sysinfo::System::new();
    let networks = Networks::new_with_refreshed_list();
    let disks = Disks::new_with_refreshed_list();
    sysinfo_sys.refresh_all();

    let cpu = cpu_info_without_usage(&sysinfo_sys);
    info!("CPU: {}, Cores: {}", cpu.name, cpu.cores);

    let mem_with_out_usage = mem_info_without_usage(&sysinfo_sys);
    let mem = realtime_mem(&sysinfo_sys);
    let swap = realtime_swap(&sysinfo_sys);
    info!(
        "Memory: {} MB / {} MB",
        mem.used / 1000 / 1000,
        mem_with_out_usage.mem / 1000 / 1000
    );
    info!(
        "Swap: {} MB / {} MB",
        swap.used / 1000 / 1000,
        mem_with_out_usage.swap / 1000 / 1000
    );

    let load = realtime_load();
    info!(
        "Load: {:.2} / {:.2} / {:.2}",
        load.load1, load.load5, load.load15
    );

    info!("");

    info!("Hard drives will be monitored:");
    let disks = filter_disks(&disks);
    for disk in disks {
        info!(
            "{} | {} | {} | {} GB / {} GB",
            disk.name().to_string_lossy(),
            disk.file_system().to_string_lossy(),
            disk.mount_point().to_string_lossy(),
            disk.available_space() / 1000 / 1000 / 1000,
            disk.total_space() / 1000 / 1000 / 1000
        );
    }

    info!("");
    info!("Network interfaces will be monitored:");
    for (name, data) in networks.iter() {
        if should_filter_interface(name, &data.mac_address().0) {
            continue;
        }
        info!(
            "{} | {} | UP: {} GB / DOWN: {} GB",
            name,
            data.mac_address().to_string(),
            data.total_transmitted() / 1000 / 1000 / 1000,
            data.total_received() / 1000 / 1000 / 1000
        );
    }
    let connections = realtime_connections();
    info!("CONNS: TCP: {} | UDP: {}", connections.tcp, connections.udp);

    info!("===== DIVIDING LINE =====")
}
