use crate::host::platform::{AppUpdateFacts, DiskFacts, DisksFacts};
use crate::platform::system::app_update::AppUpdate;

#[cfg(target_os = "linux")]
use plain_rs::platform::linux::disks::list::{Disk, read_disks};

#[cfg(target_os = "linux")]
pub fn disks() -> Result<DisksFacts, String> {
    disks_facts(read_disks())
}

pub fn app_update() -> Result<AppUpdateFacts, String> {
    Ok(update_facts(
        crate::platform::system::app_update::app_update(),
    ))
}

#[cfg(target_os = "linux")]
fn disks_facts(disks: Result<Vec<Disk>, String>) -> Result<DisksFacts, String> {
    disks.map(|disks| DisksFacts {
        disks: disks.into_iter().map(to_facts).collect(),
    })
}

fn update_facts(update: AppUpdate) -> AppUpdateFacts {
    AppUpdateFacts {
        current_version: update.current_version,
        latest_version: update.latest_version,
        has_update: update.has_update,
        url: update.url,
    }
}

#[cfg(target_os = "linux")]
fn to_facts(disk: Disk) -> DiskFacts {
    DiskFacts {
        id: disk.id,
        name: disk.name,
        path: disk.path,
        size_bytes: disk.size_bytes,
        removable: disk.removable,
        model: disk.model,
    }
}

#[cfg(not(target_os = "linux"))]
pub fn disks() -> Result<DisksFacts, String> {
    Ok(DisksFacts {
        disks: sysinfo::Disks::new_with_refreshed_list()
            .iter()
            .map(|disk| DiskFacts {
                id: disk.mount_point().to_string_lossy().into_owned(),
                name: disk.name().to_string_lossy().into_owned(),
                path: disk.mount_point().to_string_lossy().into_owned(),
                size_bytes: disk.total_space().min(i64::MAX as u64) as i64,
                removable: disk.is_removable(),
                model: None,
            })
            .collect(),
    })
}

#[cfg(test)]
#[path = "../../../../tests/unit/host/headless/native/platform.rs"]
mod tests;
