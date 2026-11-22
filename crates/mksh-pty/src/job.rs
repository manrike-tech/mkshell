//! Job Object de Windows con `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
//!
//! Al cerrar el handle del Job (cuando el panel se destruye), Windows termina
//! todos los procesos asignados a él, incluidos los nietos creados después de
//! la asignación. Los procesos creados por el shell entre `CreateProcess` y
//! `AssignProcessToJobObject` quedan fuera: es una ventana de microsegundos.
//!
//! Nota: este módulo no se ha podido compilar en el entorno de desarrollo
//! inicial (sin target Windows); se valida en la CI de Windows.

use portable_pty::Child;
use std::io;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};

pub struct Job(HANDLE);

// SAFETY: un HANDLE de Job Object puede usarse desde cualquier hilo.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    /// Crea un Job y le asigna el proceso hijo.
    pub fn for_child(child: &(dyn Child + Send + Sync)) -> io::Result<Self> {
        let process = child
            .as_raw_handle()
            .ok_or_else(|| io::Error::other("sin handle de proceso"))?;
        // SAFETY: parámetros nulos válidos según la API; se comprueban los retornos.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                CloseHandle(job);
                return Err(io::Error::last_os_error());
            }
            if AssignProcessToJobObject(job, process as HANDLE) == 0 {
                CloseHandle(job);
                return Err(io::Error::last_os_error());
            }
            Ok(Self(job))
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: handle propio, se cierra una sola vez.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
