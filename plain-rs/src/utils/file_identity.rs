use std::{io, path::Path};

pub fn key(path: &Path) -> io::Result<Option<String>> {
    let (file, created) = sample(path)?;
    Ok(created.map(|created| format!("{file}:{created}")))
}

pub fn same(left: &Path, right: &Path) -> io::Result<bool> {
    Ok(sample(left)?.0 == sample(right)?.0)
}

#[cfg(unix)]
fn sample(path: &Path) -> io::Result<(String, Option<String>)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path)?;
    let created = metadata
        .created()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|time| time.as_nanos().to_string());
    Ok((format!("{}:{}", metadata.dev(), metadata.ino()), created))
}

#[cfg(windows)]
fn sample(path: &Path) -> io::Result<(String, Option<String>)> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    #[repr(C)]
    #[derive(Default)]
    struct Information {
        attributes: u32,
        created: [u32; 2],
        accessed: [u32; 2],
        written: [u32; 2],
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(handle: *mut std::ffi::c_void, info: *mut Information)
        -> i32;
    }
    let file = std::fs::OpenOptions::new()
        .access_mode(0)
        .share_mode(7)
        .custom_flags(0x02000000)
        .open(path)?;
    let mut info = Information::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        format!("{}:{}:{}", info.volume, info.index_high, info.index_low),
        Some(format!("{}:{}", info.created[1], info.created[0])),
    ))
}

#[cfg(not(any(unix, windows)))]
fn sample(_path: &Path) -> io::Result<(String, Option<String>)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "filesystem identity unavailable",
    ))
}

#[cfg(test)]
#[path = "../../tests/unit/utils/file_identity.rs"]
mod tests;
