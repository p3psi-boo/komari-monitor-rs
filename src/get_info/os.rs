use log::trace;
use sysinfo::System;

#[derive(Debug)]
pub struct OsInfo {
    pub os: String,
    pub version: String,
    pub virtualization: String,
}

pub async fn os() -> OsInfo {
    let os = compose_os_display_name(platform_os_name(), System::os_version());
    let kernel_version = System::kernel_version().unwrap_or("Unknown".to_string());

    let virt = {
        #[cfg(target_os = "linux")]
        {
            heim_virt::detect()
                .await
                .unwrap_or(heim_virt::Virtualization::Unknown)
                .as_str()
                .to_string()
        }

        #[cfg(target_os = "windows")]
        {
            use raw_cpuid::CpuId;
            let hypervisor_present = {
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                {
                    CpuId::new()
                        .get_feature_info()
                        .is_some_and(|f| f.has_hypervisor())
                }
                #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
                {
                    false
                }
            };

            let hypervisor_vendor = {
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                {
                    if hypervisor_present {
                        CpuId::new()
                            .get_hypervisor_info()
                            .map(|hv| format!("{:?}", hv.identify()))
                    } else {
                        None
                    }
                }
                #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
                {
                    None
                }
            };

            hypervisor_vendor.unwrap_or_else(|| "Unknown".to_string())
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            "Unknown".to_string()
        }
    };

    let os_info = OsInfo {
        os,
        version: kernel_version,
        virtualization: virt,
    };

    trace!("OS INFO successfully retrieved: {os_info:?}");

    os_info
}

#[cfg(target_os = "macos")]
fn platform_os_name() -> Option<String> {
    Some("macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
fn platform_os_name() -> Option<String> {
    System::name()
}

fn compose_os_display_name(name: Option<String>, version: Option<String>) -> String {
    let name = name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unknown".to_string());
    let version = version
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty());

    match version {
        Some(version) => format!("{name} {version}"),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::compose_os_display_name;

    #[test]
    fn compose_os_display_name_includes_version_when_present() {
        assert_eq!(
            compose_os_display_name(Some("macOS".to_string()), Some("15.7".to_string())),
            "macOS 15.7"
        );
    }

    #[test]
    fn compose_os_display_name_does_not_leave_trailing_space_without_version() {
        assert_eq!(
            compose_os_display_name(Some("Linux".to_string()), None),
            "Linux"
        );
    }

    #[test]
    fn compose_os_display_name_falls_back_when_name_is_empty() {
        assert_eq!(
            compose_os_display_name(Some(" ".to_string()), Some("1.0".to_string())),
            "Unknown 1.0"
        );
    }
}
