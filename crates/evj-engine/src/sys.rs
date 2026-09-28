//! Windows tuning for a smooth show: 1 ms timers, MMCSS scheduling for the engine thread,
//! no power throttling, and no sleep / screen-off while outputs are live.
use windows::Win32::Media::timeBeginPeriod;
use windows::Win32::System::Power::{ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState};
use windows::Win32::System::Threading::{
    AvSetMmThreadCharacteristicsW, GetCurrentProcess, PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling, SetProcessInformation,
};
use windows::core::w;

/// Call once on the engine thread. Failures are logged, never fatal.
pub fn tune_engine_thread() {
    unsafe {
        let _ = timeBeginPeriod(1);
        let mut task = 0u32;
        if AvSetMmThreadCharacteristicsW(w!("Playback"), &mut task).is_err() {
            evj_core::log::warn("sys", "MMCSS 'Playback' not available");
        }
        // Opt out of EcoQoS: Windows must not slow this process down on battery / in the background.
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: 0,
        };
        if SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, (&state as *const PROCESS_POWER_THROTTLING_STATE).cast(), size_of_val(&state) as u32).is_err() {
            evj_core::log::warn("sys", "could not disable power throttling");
        }
    }
}

/// While outputs are live the laptop must not sleep or blank the screens.
pub fn keep_awake(on: bool) {
    unsafe {
        let flags = if on { ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED } else { ES_CONTINUOUS };
        SetThreadExecutionState(flags);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tuning_calls_do_not_fail() {
        super::tune_engine_thread();
        super::keep_awake(true);
        super::keep_awake(false);
    }
}
