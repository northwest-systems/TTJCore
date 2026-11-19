use crate::{PROTOCOL, ipc, store::Store};
use serde_json::json;
use std::{
    io,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

pub fn database_url(home: &Path) -> String {
    std::env::var("TTJCORE_DATABASE_URL").unwrap_or_else(|_| {
        format!(
            "sqlite://{}?mode=rwc",
            home.join("core.sqlite")
                .to_string_lossy()
                .replace('\\', "/")
        )
    })
}

async fn ready(home: &Path) -> bool {
    tokio::time::timeout(
        Duration::from_millis(500),
        ipc::call(
            home,
            json!({"jsonrpc":"2.0","protocol":PROTOCOL,"id":1,"method":"rpc.discover","params":{}}),
        ),
    )
    .await
    .is_ok_and(|result| result.is_ok_and(|response| response.get("result").is_some()))
}

struct StartupLock(std::path::PathBuf);
impl Drop for StartupLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(windows)]
fn uninherit_launcher_stdio() -> io::Result<()> {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(kind: u32) -> *mut c_void;
        fn SetHandleInformation(handle: *mut c_void, mask: u32, flags: u32) -> i32;
    }
    // CREATE_NO_WINDOW still inherits the launcher's original pipe handles.
    // Keeping those handles in the daemon prevents callers from receiving EOF.
    for kind in [-10i32, -11, -12] {
        // SAFETY: these are the documented standard-handle selectors; invalid
        // handles are excluded before calling SetHandleInformation.
        let handle = unsafe { GetStdHandle(kind as u32) };
        if !handle.is_null()
            && handle as isize != -1
            && unsafe { SetHandleInformation(handle, 1, 0) } == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Starts the shared service only when this profile has no running Core.
pub async fn ensure(home: &Path) -> io::Result<()> {
    ipc::prepare_home(home)?;
    if ready(home).await {
        return Ok(());
    }
    let lock_path = home.join("startup.lock");
    let mut guard = None;
    for _ in 0..100 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => {
                guard = Some(StartupLock(lock_path.clone()));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if ready(home).await {
                    return Ok(());
                }
                if std::fs::metadata(&lock_path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age > Duration::from_secs(60))
                {
                    let _ = std::fs::remove_file(&lock_path);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => return Err(error),
        }
    }
    let _guard = guard.ok_or_else(|| io::Error::other("Core startup is busy; try again"))?;
    if ready(home).await {
        return Ok(());
    }
    Store::connect(&database_url(home), true)
        .await
        .map_err(|_| {
            io::Error::other("Cannot initialize Core database; check database settings")
        })?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--home")
        .arg(home)
        .arg("serve")
        .env("TTJCORE_DATABASE_URL", database_url(home))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        uninherit_launcher_stdio()?;
        command.creation_flags(0x08000000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    for _ in 0..100 {
        if ready(home).await {
            return Ok(());
        }
        if child.try_wait()?.is_some() {
            return Err(io::Error::other(
                "Core could not start; run serve to inspect configuration",
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(io::Error::other("Core startup timed out"))
}
