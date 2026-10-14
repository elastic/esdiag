use eyre::{Result, eyre};
use std::{io, path::Path};
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, FILETIME, HANDLE, WAIT_OBJECT_0},
    System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
        QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
    },
};

/// Keep the same process handle for identity checks and termination so a reused
/// PID cannot cause an unrelated process to be stopped.
pub(super) struct Process(HANDLE);

impl Process {
    pub(super) fn open(pid: i32, terminate: bool) -> Result<Option<Self>> {
        if pid <= 0 {
            return Ok(None);
        }
        let rights =
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | if terminate { PROCESS_TERMINATE } else { 0 };
        let handle = unsafe { OpenProcess(rights, 0, pid as u32) };
        if handle.is_null() {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                return Ok(None);
            }
            return Err(error.into());
        }
        let process = Self(handle);
        if unsafe { WaitForSingleObject(handle, 0) } == WAIT_OBJECT_0 {
            return Ok(None);
        }
        Ok(Some(process))
    }

    pub(super) fn started(&self) -> Result<String> {
        let mut creation: FILETIME = unsafe { std::mem::zeroed() };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        if unsafe { GetProcessTimes(self.0, &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        Ok((((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64).to_string())
    }

    pub(super) fn matches(&self, binary: &str, started: &str) -> Result<bool> {
        if binary.trim().is_empty() || started.trim().is_empty() || self.started()? != started.trim() {
            return Ok(false);
        }
        let mut image = vec![0u16; 32768];
        let mut length = image.len() as u32;
        if unsafe { QueryFullProcessImageNameW(self.0, 0, image.as_mut_ptr(), &mut length) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let image = String::from_utf16(&image[..length as usize])?;
        let actual = std::fs::canonicalize(Path::new(&image))?;
        let expected = std::fs::canonicalize(Path::new(binary.trim()))?;
        Ok(actual
            .to_string_lossy()
            .eq_ignore_ascii_case(&expected.to_string_lossy()))
    }

    pub(super) fn stop(&self) -> Result<()> {
        if unsafe { TerminateProcess(self.0, 0) } == 0 {
            if unsafe { WaitForSingleObject(self.0, 0) } == WAIT_OBJECT_0 {
                return Ok(());
            }
            return Err(io::Error::last_os_error().into());
        }
        if unsafe { WaitForSingleObject(self.0, 5000) } != WAIT_OBJECT_0 {
            return Err(eyre!("Managed native ESDiag service did not stop within five seconds"));
        }
        Ok(())
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Process;
    use std::{
        os::windows::process::CommandExt,
        process::{Child, Command, Stdio},
    };

    struct TestChild(Child);
    impl Drop for TestChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn windows_process_identity_and_shutdown() {
        let binary = std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap()).join("System32/cmd.exe");
        let mut child = TestChild(
            Command::new(&binary)
                .args(["/C", "ping -n 30 127.0.0.1 >nul"])
                .creation_flags(0x08000000)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let process = Process::open(child.0.id() as i32, true).unwrap().unwrap();
        let started = process.started().unwrap();
        assert!(process.matches(binary.to_str().unwrap(), &started).unwrap());
        assert!(!process.matches(binary.to_str().unwrap(), "wrong-start-time").unwrap());
        assert!(
            !process
                .matches(std::env::current_exe().unwrap().to_str().unwrap(), &started)
                .unwrap()
        );
        assert!(!process.matches("", &started).unwrap());
        assert!(child.0.try_wait().unwrap().is_none());
        process.stop().unwrap();
        assert!(child.0.wait().unwrap().success());
        assert!(Process::open(child.0.id() as i32, false).unwrap().is_none());
    }
}
