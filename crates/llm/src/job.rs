//! Ties the sidecar's lifetime to ours on Windows. Without this, a crash or
//! a killed parent leaves `llama-server` running and holding VRAM, because
//! Windows does not terminate children with their parent.

use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
};

/// Job object configured to kill its processes when the last handle closes.
#[derive(Debug)]
pub struct KillOnClose(HANDLE);

// SAFETY: a job handle is a kernel object reference usable from any thread.
unsafe impl Send for KillOnClose {}
unsafe impl Sync for KillOnClose {}

impl KillOnClose {
    pub fn assign(process: std::os::windows::io::RawHandle) -> windows::core::Result<Self> {
        // SAFETY: `info` outlives the call and its size is passed explicitly;
        // the job handle is owned by the returned guard.
        unsafe {
            let job = CreateJobObjectW(None, None)?;
            let guard = Self(job);
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast::<c_void>(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
            AssignProcessToJobObject(job, HANDLE(process))?;
            Ok(guard)
        }
    }
}

impl Drop for KillOnClose {
    fn drop(&mut self) {
        // SAFETY: the handle was created by `CreateJobObjectW` and is closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
