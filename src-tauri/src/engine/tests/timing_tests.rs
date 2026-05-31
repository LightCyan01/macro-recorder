use crate::{
    app_state::AppState,
    engine::{player::wait_until, worker::RunControl, TimerResolutionGuard},
    hotkeys::HotkeyConfig,
};
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use windows::{
    core::{s, PCSTR},
    Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
};

type NtSetTimerResolutionFn = unsafe extern "system" fn(u32, u8, *mut u32) -> u32;
type NtQueryTimerResolutionFn = unsafe extern "system" fn(*mut u32, *mut u32, *mut u32) -> u32;

#[cfg(target_os = "windows")]
#[test]
fn nt_set_timer_resolution_sets_and_restores() {
    let (minimum, maximum, before_current) = query_timer_resolution();

    let mut set_current = 0u32;
    let set_timer_resolution = nt_set_timer_resolution();
    let set_status = unsafe { set_timer_resolution(5000, 1, &mut set_current) };
    let (_, _, queried_set_current) = query_timer_resolution();

    let mut restored_current = 0u32;
    let restore_status = unsafe { set_timer_resolution(5000, 0, &mut restored_current) };
    let (_, _, after_restore_current) = query_timer_resolution();

    println!(
        "timer resolution 100ns units: min={minimum}, max={maximum}, before={before_current}, set_return={set_current}, queried_set={queried_set_current}, restore_return={restored_current}, after_restore={after_restore_current}, set_status={set_status}, restore_status={restore_status}"
    );

    assert_eq!(set_status, 0, "NtSetTimerResolution set failed");
    assert_eq!(restore_status, 0, "NtSetTimerResolution restore failed");
    assert!(
        set_current > 0,
        "set call did not report a current resolution"
    );
    assert!(
        queried_set_current <= before_current,
        "requested timer resolution should not be coarser than the pre-test resolution"
    );
    assert!(
        after_restore_current >= queried_set_current,
        "restore should leave the system no finer than the requested test resolution"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn spin_wait_accuracy_1000_iterations_1ms_target() {
    let _timer = TimerResolutionGuard::request_500us();
    let (control, _active) = test_control();
    let mut errors = Vec::with_capacity(1000);

    for _ in 0..1000 {
        let target = Instant::now() + Duration::from_millis(1);
        wait_until(target, &control);
        errors.push(signed_error_ms(Instant::now(), target).abs());
    }

    let (mean, max, stddev) = stats(&errors);
    println!(
        "spin wait 1ms x1000: mean_error_ms={mean:.6}, max_error_ms={max:.6}, stddev_ms={stddev:.6}"
    );

    assert!(mean < 0.5, "mean spin-wait error was {mean:.6}ms");
    assert!(max < 1.5, "max spin-wait error was {max:.6}ms");
    assert!(stddev < 0.3, "spin-wait stddev was {stddev:.6}ms");
}

#[cfg(target_os = "windows")]
#[test]
fn absolute_deadline_loop_has_no_cumulative_drift() {
    let _timer = TimerResolutionGuard::request_500us();
    let (control, _active) = test_control();
    let interval = Duration::from_millis(16);
    let start = Instant::now();
    let mut lateness = Vec::with_capacity(100);

    for event_index in 1..=100 {
        let target = start + interval * event_index;
        wait_until(target, &control);
        lateness.push(signed_error_ms(Instant::now(), target));
    }

    let max_late = lateness.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let final_drift = lateness.last().copied().unwrap_or_default();
    println!(
        "absolute deadline 100 events @16ms: max_late_ms={max_late:.6}, final_drift_ms={final_drift:.6}"
    );

    assert!(
        max_late <= 2.0,
        "at least one event was more than 2ms late: {max_late:.6}ms"
    );
    assert!(
        final_drift.abs() < 2.0,
        "final event drift should stay under 2ms, got {final_drift:.6}ms"
    );
}

#[cfg(target_os = "windows")]
#[test]
fn relative_sleep_drift_fails_while_absolute_deadline_passes() {
    let interval = Duration::from_millis(16);
    let iterations = 50u32;

    let relative_start = Instant::now();
    let mut relative_lateness = Vec::with_capacity(iterations as usize);
    for event_index in 1..=iterations {
        thread::sleep(interval);
        let target = relative_start + interval * event_index;
        relative_lateness.push(signed_error_ms(Instant::now(), target));
    }
    let relative_max_late = relative_lateness
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let relative_final_drift = relative_lateness.last().copied().unwrap_or_default();

    let _timer = TimerResolutionGuard::request_500us();
    let (control, _active) = test_control();
    let absolute_start = Instant::now();
    let mut absolute_lateness = Vec::with_capacity(iterations as usize);
    for event_index in 1..=iterations {
        let target = absolute_start + interval * event_index;
        wait_until(target, &control);
        absolute_lateness.push(signed_error_ms(Instant::now(), target));
    }
    let absolute_max_late = absolute_lateness
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let absolute_final_drift = absolute_lateness.last().copied().unwrap_or_default();

    println!(
        "relative sleep drift: max_late_ms={relative_max_late:.6}, final_drift_ms={relative_final_drift:.6}; absolute deadline drift: max_late_ms={absolute_max_late:.6}, final_drift_ms={absolute_final_drift:.6}"
    );

    let relative_passes = relative_max_late <= 2.0 && relative_final_drift.abs() < 2.0;
    let absolute_passes = absolute_max_late <= 2.0 && absolute_final_drift.abs() < 2.0;

    assert!(
        absolute_passes,
        "absolute deadline loop should pass drift limits"
    );
    assert!(
        !relative_passes,
        "relative sleep unexpectedly passed drift limits; relative_final_drift_ms={relative_final_drift:.6}"
    );
}

fn query_timer_resolution() -> (u32, u32, u32) {
    let query_timer_resolution = nt_query_timer_resolution();
    let mut minimum = 0u32;
    let mut maximum = 0u32;
    let mut current = 0u32;
    let status = unsafe { query_timer_resolution(&mut minimum, &mut maximum, &mut current) };
    assert_eq!(status, 0, "NtQueryTimerResolution failed");
    (minimum, maximum, current)
}

fn nt_set_timer_resolution() -> NtSetTimerResolutionFn {
    unsafe { std::mem::transmute(resolve_ntdll_symbol("NtSetTimerResolution")) }
}

fn nt_query_timer_resolution() -> NtQueryTimerResolutionFn {
    unsafe { std::mem::transmute(resolve_ntdll_symbol("NtQueryTimerResolution")) }
}

unsafe fn resolve_ntdll_symbol(name: &str) -> *const c_void {
    let module = GetModuleHandleA(s!("ntdll.dll")).expect("ntdll.dll should be loaded");
    let mut symbol_name = name.as_bytes().to_vec();
    symbol_name.push(0);
    let symbol = GetProcAddress(module, PCSTR(symbol_name.as_ptr()));
    let Some(symbol) = symbol else {
        panic!("{name} was not exported by ntdll.dll");
    };
    symbol as *const c_void
}

fn test_control() -> (RunControl, Arc<AtomicBool>) {
    let state = Arc::new(AppState::new(HotkeyConfig::default()));
    let active = Arc::new(AtomicBool::new(true));
    let generation = state.run_gen.load(Ordering::Acquire);
    (RunControl::new(state, generation, active.clone()), active)
}

fn signed_error_ms(actual: Instant, target: Instant) -> f64 {
    if actual >= target {
        actual.duration_since(target).as_secs_f64() * 1000.0
    } else {
        -(target.duration_since(actual).as_secs_f64() * 1000.0)
    }
}

fn stats(values: &[f64]) -> (f64, f64, f64) {
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let variance = values
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f64>()
        / values.len() as f64;
    (mean, max, variance.sqrt())
}
