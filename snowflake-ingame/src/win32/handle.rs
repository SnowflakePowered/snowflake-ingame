use windows::Win32::Foundation::{
    DuplicateHandle, GetLastError, DUPLICATE_SAME_ACCESS, HANDLE, WIN32_ERROR,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE};

#[derive(thiserror::Error, Debug)]
pub enum DuplicateHandleError {
    #[error("Unable to open source process")]
    InvalidProcess,
    #[error("Unable to duplicate handle: {0:x?}")]
    CannotDuplicate(WIN32_ERROR),
}

pub fn duplicate_handle(source_pid: u32, handle: HANDLE) -> Result<HANDLE, DuplicateHandleError> {
    let process = unsafe { OpenProcess(PROCESS_DUP_HANDLE, false, source_pid) };
    if process.is_invalid() {
        return Err(DuplicateHandleError::InvalidProcess);
    }

    let mut duped_handle = HANDLE::default();
    if !(unsafe {
        DuplicateHandle(
            process,
            handle,
            GetCurrentProcess(),
            &mut duped_handle,
            0,
            false,
            DUPLICATE_SAME_ACCESS,
        )
    }.as_bool())
    {
        let err = unsafe { GetLastError() };
        return Err(DuplicateHandleError::CannotDuplicate(err));
    }

    Ok(duped_handle)
}
