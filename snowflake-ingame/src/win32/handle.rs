use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, GetLastError, DUPLICATE_SAME_ACCESS, HANDLE, WIN32_ERROR,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE};

#[derive(thiserror::Error, Debug)]
pub enum HandleError {
    #[error("Unable to open source process")]
    InvalidProcess,
    #[error("Unable to duplicate handle: {0:x?}")]
    CannotDuplicate(WIN32_ERROR),
    #[error("Unable to close handle {0:x?}")]
    CannotClose(WIN32_ERROR),
}

pub fn try_duplicate_handle(source_pid: u32, handle: HANDLE) -> Result<HANDLE, HandleError> {
    let process = unsafe {
        OpenProcess(PROCESS_DUP_HANDLE, false, source_pid)
            .map_err(|_| HandleError::InvalidProcess)?
    };
    if process.is_invalid() {
        return Err(HandleError::InvalidProcess);
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
    }
    .as_bool())
    {
        let err = unsafe { GetLastError() };
        return Err(HandleError::CannotDuplicate(err));
    }

    Ok(duped_handle)
}

pub fn try_close_handle(handle: HANDLE) -> Result<(), HandleError> {
    if !(unsafe { CloseHandle(handle) }.as_bool()) {
        let error = unsafe { GetLastError() };
        return Err(HandleError::CannotClose(error));
    }
    Ok(())
}
