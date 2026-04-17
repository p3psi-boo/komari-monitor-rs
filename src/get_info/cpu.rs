use crate::data_struct::Cpu;
use log::trace;
use sysinfo::System;

pub fn arch() -> String {
    let arch = std::env::consts::ARCH.to_string();
    trace!("ARCH successfully retrieved: {arch}");
    arch
}

#[derive(Debug)]
pub struct CPUInfoWithOutUsage {
    pub name: String,
    pub cores: u16,
}

pub fn cpu_info_without_usage(sysinfo_sys: &System) -> CPUInfoWithOutUsage {
    let cores = u16::try_from(sysinfo_sys.cpus().len()).unwrap_or(0);
    let mut seen = std::collections::HashSet::new();
    // Collect brands in the order they first appear, deduplicated
    let mut ordered_brands: Vec<String> = Vec::new();
    for cpu in sysinfo_sys.cpus() {
        let brand = cpu.brand().to_string();
        if seen.insert(brand.clone()) {
            ordered_brands.push(brand);
        }
    }
    // Stable ordering: sort alphabetically after deduplication
    ordered_brands.sort();
    let name = ordered_brands.join(", ").trim().to_string();

    let cpu_info = CPUInfoWithOutUsage { name, cores };

    trace!("CPU INFO WITH OUT USAGE successfully retrieved: {cpu_info:?}");

    cpu_info
}

pub fn realtime_cpu(sysinfo_sys: &System) -> Cpu {
    let cpus = sysinfo_sys.cpus();
    // Return 0.0 when no CPUs are available to avoid division by zero
    if cpus.is_empty() {
        return Cpu { usage: 0.0 };
    }
    let mut avg = 0.0;
    for cpu in cpus {
        avg += cpu.cpu_usage();
    }
    let avg = f64::from(avg) / cpus.len() as f64;

    let cpu = Cpu { usage: avg };
    trace!("REALTIME CPU successfully retrieved: {cpu:?}");
    cpu
}
