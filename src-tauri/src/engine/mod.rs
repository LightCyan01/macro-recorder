pub mod input;
pub mod player;
pub mod recorder;
pub mod worker;

#[cfg(test)]
mod tests;

#[cfg(windows)]
use std::{ffi::c_void, sync::OnceLock};

#[cfg(windows)]
use windows::{
    core::{s, PCSTR},
    Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
};

#[cfg(windows)]
type NtSetTimerResolutionFn = unsafe extern "system" fn(u32, u8, *mut u32) -> u32;

pub struct TimerResolutionGuard {
    current: u32,
    active: bool,
}

impl TimerResolutionGuard {
    pub fn request_500us() -> Self {
        let mut current = 0u32;

        #[cfg(windows)]
        let active = nt_set_timer_resolution(5000, 1, &mut current) == 0;

        #[cfg(not(windows))]
        let active = false;

        Self { current, active }
    }
}

impl Drop for TimerResolutionGuard {
    fn drop(&mut self) {
        if self.active {
            #[cfg(windows)]
            let _ = nt_set_timer_resolution(5000, 0, &mut self.current);
        }
    }
}

#[cfg(windows)]
fn nt_set_timer_resolution(desired_resolution: u32, set_resolution: u8, current: *mut u32) -> u32 {
    static NT_SET_TIMER_RESOLUTION: OnceLock<Option<NtSetTimerResolutionFn>> = OnceLock::new();

    match NT_SET_TIMER_RESOLUTION.get_or_init(resolve_nt_set_timer_resolution) {
        Some(function) => unsafe { function(desired_resolution, set_resolution, current) },
        None => u32::MAX,
    }
}

#[cfg(windows)]
fn resolve_nt_set_timer_resolution() -> Option<NtSetTimerResolutionFn> {
    unsafe {
        let module = GetModuleHandleA(s!("ntdll.dll")).ok()?;
        let symbol = GetProcAddress(module, PCSTR(b"NtSetTimerResolution\0".as_ptr()))?;
        Some(std::mem::transmute::<*const c_void, NtSetTimerResolutionFn>(symbol as *const c_void))
    }
}
