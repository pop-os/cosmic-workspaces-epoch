use rustix::{io::Errno, shm::ShmOFlags};
use std::{
    os::fd::OwnedFd,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "linux")]
fn create_memfd() -> rustix::io::Result<OwnedFd> {
    let fd = rustix::io::retry_on_intr(|| {
        rustix::fs::memfd_create(
            "cosmic-workspaces-shm",
            rustix::fs::MemfdFlags::CLOEXEC | rustix::fs::MemfdFlags::ALLOW_SEALING,
        )
    })?;
    let _ = rustix::fs::fcntl_add_seals(
        &fd,
        rustix::fs::SealFlags::SHRINK | rustix::fs::SealFlags::SEAL,
    );
    Ok(fd)
}

pub fn create_memfile() -> rustix::io::Result<OwnedFd> {
    #[cfg(target_os = "linux")]
    if let Ok(fd) = create_memfd() {
        return Ok(fd);
    }

    loop {
        let flags = ShmOFlags::CREATE | ShmOFlags::EXCL | ShmOFlags::RDWR;

        let time = SystemTime::now();
        let name = format!(
            "/cosmic-workspaces-shm-{}",
            time.duration_since(UNIX_EPOCH).unwrap().subsec_nanos()
        );

        match rustix::shm::shm_open(&name, flags, 0600.into()) {
            Ok(fd) => match rustix::shm::shm_unlink(&name) {
                Ok(_) => return Ok(fd),
                Err(errno) => {
                    return Err(errno.into());
                }
            },
            #[allow(unreachable_patterns)]
            Err(Errno::EXIST | Errno::EXIST) => {
                continue;
            }
            Err(err) => return Err(err.into()),
        }
    }
}
