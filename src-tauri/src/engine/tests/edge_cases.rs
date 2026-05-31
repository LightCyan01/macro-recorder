use crate::{
    app_state::{AppState, PlaybackOptions},
    engine::{
        input::simulated_cursor_drive,
        player::{
            is_playing_macro, move_cursor_to_with, playback_events_for_test,
            should_dispatch_keyboard_event, PlaybackCursor, PlaybackHeldInputs,
        },
        recorder::{
            append_missing_keyups, record_keyboard_event_for_test,
            record_keyboard_transition_for_test, reset_recording_clock_for_test,
        },
        worker::RunControl,
        TimerResolutionGuard,
    },
    hotkeys::HotkeyConfig,
    macro_file::{prepare_macro_for_save, MacroEvent, MacroFile, MouseButton},
};
use std::{
    cell::Cell,
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[test]
fn empty_macro_zero_events_plays_without_panic() {
    let (control, _active) = test_control();
    let macro_file = MacroFile::new("empty", Vec::new());

    playback_events_for_test(
        &macro_file.events,
        PlaybackOptions::default(),
        &control,
        |_, _, _, _| {
            panic!("empty macro should not dispatch events");
        },
    )
    .expect("empty macro playback should return cleanly");
}

#[test]
fn single_zero_elapsed_event_fires_within_5ms() {
    let (control, _active) = test_control();
    let events = vec![MacroEvent::KeyDown {
        vk_code: 0x41,
        elapsed_ms: 0.0,
    }];
    let mut fired_ms = Vec::new();

    playback_events_for_test(
        &events,
        PlaybackOptions::default(),
        &control,
        |_, _, start, fired| {
            fired_ms.push(fired.duration_since(start).as_secs_f64() * 1000.0);
            Ok(())
        },
    )
    .expect("single event playback should complete");

    println!("single zero-elapsed event fired_ms={:.6}", fired_ms[0]);
    assert!(fired_ms[0] <= 5.0, "event fired too late");
}

#[test]
fn ten_thousand_event_macro_json_round_trip_preserves_fields() {
    let events = (0..10_000)
        .map(|index| MacroEvent::MouseMove {
            x: index,
            y: -index,
            elapsed_ms: index as f64 * 0.125,
        })
        .collect::<Vec<_>>();
    let macro_file = MacroFile {
        name: "large macro".to_string(),
        created_at: "2026-05-14T18:00:00.000Z".to_string(),
        duration_ms: 1249.875,
        events,
    };

    let json = serde_json::to_string(&macro_file).expect("macro should serialize");
    let decoded: MacroFile = serde_json::from_str(&json).expect("macro should deserialize");

    println!(
        "large macro json bytes={}, decoded_events={}",
        json.len(),
        decoded.events.len()
    );
    assert_eq!(decoded.events.len(), 10_000);
    assert_eq!(decoded, macro_file);
}

#[test]
fn macro_new_materializes_leading_recording_delay_as_command() {
    let macro_file = MacroFile::from_recording(
        "leading delay",
        vec![
            MacroEvent::KeyDown {
                vk_code: 0x41,
                elapsed_ms: 325.0,
            },
            MacroEvent::KeyUp {
                vk_code: 0x41,
                elapsed_ms: 350.0,
            },
        ],
        400.0,
    );

    println!(
        "materialized leading delay events={:?}, duration_ms={}",
        macro_file.events, macro_file.duration_ms
    );
    assert_eq!(macro_file.duration_ms, 400.0);
    assert_eq!(
        macro_file.events,
        vec![
            MacroEvent::Delay {
                duration_ms: 325.0,
                elapsed_ms: 325.0
            },
            MacroEvent::KeyDown {
                vk_code: 0x41,
                elapsed_ms: 325.0
            },
            MacroEvent::Delay {
                duration_ms: 25.0,
                elapsed_ms: 350.0
            },
            MacroEvent::KeyUp {
                vk_code: 0x41,
                elapsed_ms: 350.0
            },
            MacroEvent::Delay {
                duration_ms: 50.0,
                elapsed_ms: 400.0
            }
        ]
    );
}

#[test]
fn empty_recording_becomes_delay_only_macro() {
    let macro_file = MacroFile::from_recording("idle only", Vec::new(), 750.0);

    println!("idle-only macro events={:?}", macro_file.events);
    assert_eq!(macro_file.duration_ms, 750.0);
    assert_eq!(
        macro_file.events,
        vec![MacroEvent::Delay {
            duration_ms: 750.0,
            elapsed_ms: 750.0
        }]
    );
}

#[test]
fn stop_signal_cancels_playback_within_200ms_without_deadlock() {
    let (control, active) = test_control();
    let events = vec![MacroEvent::KeyDown {
        vk_code: 0x41,
        elapsed_ms: 5_000.0,
    }];
    let (done_tx, done_rx) = mpsc::channel();

    thread::spawn(move || {
        let result = playback_events_for_test(
            &events,
            PlaybackOptions::default(),
            &control,
            |_, _, _, _| Ok(()),
        );
        let _ = done_tx.send(result);
    });

    thread::sleep(Duration::from_millis(25));
    let cancel_started = Instant::now();
    active.store(false, Ordering::Release);
    let result = done_rx
        .recv_timeout(Duration::from_millis(200))
        .expect("playback worker should observe the stop signal promptly");
    let cancel_ms = cancel_started.elapsed().as_secs_f64() * 1000.0;

    println!("stop signal cancellation latency_ms={cancel_ms:.6}");
    result.expect("playback helper should exit without dispatch errors");
    assert!(cancel_ms <= 200.0, "cancellation took {cancel_ms:.6}ms");
}

#[test]
fn hotkey_vk_codes_are_suppressed_from_recording_buffer() {
    let state = Arc::new(AppState::new(HotkeyConfig::default()));
    state.recording.store(true, Ordering::Release);
    reset_recording_clock_for_test(&state);

    record_keyboard_event_for_test(&state, 0x78);
    record_keyboard_event_for_test(&state, 0x79);
    record_keyboard_event_for_test(&state, 0x7A);
    record_keyboard_event_for_test(&state, 0x41);

    let events = state.events_buf.lock().expect("events buffer").clone();
    println!("hotkey suppression recorded_events={events:?}");

    assert_eq!(events.len(), 1);
    assert!(matches!(
        events[0],
        MacroEvent::KeyDown { vk_code: 0x41, .. }
    ));
}

#[test]
fn recording_suppresses_duplicate_keydown_until_keyup() {
    let state = Arc::new(AppState::new(HotkeyConfig::default()));
    state.recording.store(true, Ordering::Release);
    reset_recording_clock_for_test(&state);

    record_keyboard_transition_for_test(&state, 0x41, true);
    record_keyboard_transition_for_test(&state, 0x41, true);
    record_keyboard_transition_for_test(&state, 0x41, false);
    record_keyboard_transition_for_test(&state, 0x41, true);

    let events = state.events_buf.lock().expect("events buffer").clone();
    println!("deduped recording keyboard events={events:?}");

    assert_eq!(events.len(), 3);
    assert!(matches!(
        events[0],
        MacroEvent::KeyDown { vk_code: 0x41, .. }
    ));
    assert!(matches!(events[1], MacroEvent::KeyUp { vk_code: 0x41, .. }));
    assert!(matches!(
        events[2],
        MacroEvent::KeyDown { vk_code: 0x41, .. }
    ));
}

#[test]
fn stopping_recording_synthesizes_keyups_for_keys_still_held() {
    let mut events = vec![MacroEvent::KeyDown {
        vk_code: 0x57,
        elapsed_ms: 120.0,
    }];
    let mut keys_down = HashSet::from([0x57, 0x41]);

    append_missing_keyups(&mut events, &mut keys_down, 900.0);

    println!("recording stop synthesized events={events:?}");
    assert!(keys_down.is_empty());
    assert_eq!(
        events,
        vec![
            MacroEvent::KeyDown {
                vk_code: 0x57,
                elapsed_ms: 120.0
            },
            MacroEvent::KeyUp {
                vk_code: 0x41,
                elapsed_ms: 900.0
            },
            MacroEvent::KeyUp {
                vk_code: 0x57,
                elapsed_ms: 900.0
            }
        ]
    );
}

#[test]
fn playback_suppresses_duplicate_keydown_and_stray_keyup() {
    let mut keys_down = std::collections::HashSet::new();
    let events = [
        MacroEvent::KeyDown {
            vk_code: 0x41,
            elapsed_ms: 0.0,
        },
        MacroEvent::KeyDown {
            vk_code: 0x41,
            elapsed_ms: 1.0,
        },
        MacroEvent::KeyUp {
            vk_code: 0x41,
            elapsed_ms: 2.0,
        },
        MacroEvent::KeyUp {
            vk_code: 0x41,
            elapsed_ms: 3.0,
        },
    ];
    let dispatch = events
        .iter()
        .map(|event| should_dispatch_keyboard_event(&mut keys_down, event))
        .collect::<Vec<_>>();

    println!("playback keyboard dispatch decisions={dispatch:?}");
    assert_eq!(dispatch, vec![true, false, true, false]);
}

#[test]
fn playback_stop_releases_held_keys_and_mouse_buttons() {
    let mut held = PlaybackHeldInputs::default();
    assert!(held.key_down(0x57));
    assert!(held.mouse_down(MouseButton::Left));

    let mut released_keys = Vec::new();
    let mut released_buttons = Vec::new();
    held.release_all_with(
        |vk_code, down| {
            released_keys.push((vk_code, down));
            Ok(())
        },
        |button, down| {
            released_buttons.push((button, down));
            Ok(())
        },
    )
    .expect("held input cleanup should succeed");

    println!("released_keys={released_keys:?}, released_buttons={released_buttons:?}");
    assert_eq!(released_keys, vec![(0x57, false)]);
    assert_eq!(released_buttons, vec![(MouseButton::Left, false)]);

    released_keys.clear();
    released_buttons.clear();
    held.release_all_with(
        |vk_code, down| {
            released_keys.push((vk_code, down));
            Ok(())
        },
        |button, down| {
            released_buttons.push((button, down));
            Ok(())
        },
    )
    .expect("second cleanup should be a no-op");
    assert!(released_keys.is_empty());
    assert!(released_buttons.is_empty());
}

#[test]
fn fast_cursor_movement_uses_reported_cursor_for_corrections() {
    let (final_cursor, deltas) = simulated_cursor_drive((0, 0), (100, 60), 2);

    println!("simulated cursor correction final={final_cursor:?}, deltas={deltas:?}");
    assert_eq!(final_cursor, (100, 60));
    assert!(
        deltas.len() > 1,
        "fast movement correction should issue follow-up deltas"
    );
    assert!(
        deltas
            .windows(2)
            .all(|window| window[1].0.abs() <= window[0].0.abs()
                && window[1].1.abs() <= window[0].1.abs()),
        "correction deltas should converge toward the target"
    );
}

#[test]
fn playback_anchors_first_mouse_event_without_sending_monitor_jump_delta() {
    let mut cursor = PlaybackCursor::new(Some((3200, 180)));
    let actual_cursor = Cell::new((3200, 180));
    let mut positioned = Vec::new();
    let mut activated = Vec::new();
    let mut relative_deltas = Vec::new();

    move_cursor_to_with(
        640,
        360,
        &mut cursor,
        |x, y| {
            positioned.push((x, y));
            actual_cursor.set((x, y));
            Ok(())
        },
        |x, y| activated.push((x, y)),
        || Some(actual_cursor.get()),
        |dx, dy| {
            relative_deltas.push((dx, dy));
            let current = actual_cursor.get();
            actual_cursor.set((current.0 + dx, current.1 + dy));
            Ok(())
        },
    )
    .expect("first mouse event should anchor cursor");

    println!(
        "playback cursor anchor positioned={positioned:?}, activated={activated:?}, relative_deltas={relative_deltas:?}, cursor={cursor:?}"
    );
    assert_eq!(positioned, vec![(640, 360)]);
    assert_eq!(activated, vec![(640, 360)]);
    assert!(
        relative_deltas.is_empty(),
        "first event must not inject the second-monitor-to-game relative delta"
    );
    assert_eq!(cursor.current(), Some((640, 360)));
}

#[test]
fn playback_uses_relative_deltas_after_first_mouse_anchor() {
    let mut cursor = PlaybackCursor::new(Some((3200, 180)));
    let actual_cursor = Cell::new((3200, 180));
    let mut positioned = Vec::new();
    let mut relative_deltas = Vec::new();

    move_cursor_to_with(
        640,
        360,
        &mut cursor,
        |x, y| {
            positioned.push((x, y));
            actual_cursor.set((x, y));
            Ok(())
        },
        |_, _| {},
        || Some(actual_cursor.get()),
        |dx, dy| {
            relative_deltas.push((dx, dy));
            let current = actual_cursor.get();
            actual_cursor.set((current.0 + dx, current.1 + dy));
            Ok(())
        },
    )
    .expect("first mouse event should anchor cursor");

    move_cursor_to_with(
        650,
        355,
        &mut cursor,
        |x, y| {
            positioned.push((x, y));
            actual_cursor.set((x, y));
            Ok(())
        },
        |_, _| {},
        || Some(actual_cursor.get()),
        |dx, dy| {
            relative_deltas.push((dx, dy));
            let current = actual_cursor.get();
            actual_cursor.set((current.0 + dx, current.1 + dy));
            Ok(())
        },
    )
    .expect("subsequent mouse events should use normal relative playback");

    println!(
        "post-anchor cursor positioned={positioned:?}, relative_deltas={relative_deltas:?}, cursor={cursor:?}"
    );
    assert_eq!(positioned, vec![(640, 360)]);
    assert_eq!(relative_deltas, vec![(10, -5)]);
    assert_eq!(cursor.current(), Some((650, 355)));
}

#[test]
fn all_macro_event_variants_survive_json_round_trip() {
    let events = vec![
        MacroEvent::Delay {
            duration_ms: 10.0,
            elapsed_ms: 10.0,
        },
        MacroEvent::MouseMove {
            x: 12,
            y: -8,
            elapsed_ms: 1.25,
        },
        MacroEvent::MouseDown {
            button: MouseButton::Left,
            x: 13,
            y: 14,
            elapsed_ms: 2.5,
        },
        MacroEvent::MouseUp {
            button: MouseButton::Right,
            x: -15,
            y: 16,
            elapsed_ms: 3.75,
        },
        MacroEvent::MouseScroll {
            delta: -120,
            elapsed_ms: 4.0,
        },
        MacroEvent::KeyDown {
            vk_code: 0x10,
            elapsed_ms: 5.5,
        },
        MacroEvent::KeyUp {
            vk_code: 0x10,
            elapsed_ms: 6.75,
        },
    ];

    let json = serde_json::to_string(&events).expect("events should serialize");
    let decoded: Vec<MacroEvent> = serde_json::from_str(&json).expect("events should deserialize");

    println!("all variants json={json}");
    assert_eq!(decoded, events);
}

#[cfg(target_os = "windows")]
#[test]
fn speed_multiplier_scales_event_timestamps() {
    let _timer = TimerResolutionGuard::request_500us();
    let fast = run_scaled_playback(2.0);
    let slow = run_scaled_playback(0.5);

    println!("speed 2x fired_ms={fast:?}; speed 0.5x fired_ms={slow:?}");
    assert_scaled(&fast, &[10.0, 20.0]);
    assert_scaled(&slow, &[40.0, 80.0]);
}

#[cfg(target_os = "windows")]
#[test]
fn infinite_loop_repeats_until_stop_signal() {
    let _timer = TimerResolutionGuard::request_500us();
    let (control, active) = test_control();
    let events = vec![MacroEvent::KeyDown {
        vk_code: 0x41,
        elapsed_ms: 0.0,
    }];
    let mut fired = 0usize;

    playback_events_for_test(
        &events,
        PlaybackOptions {
            loop_count: 1,
            speed_multiplier: 1.0,
            infinite_loop: true,
        },
        &control,
        |_, _, _, _| {
            fired += 1;
            if fired == 3 {
                active.store(false, Ordering::Release);
            }
            Ok(())
        },
    )
    .expect("infinite playback should stop cleanly when cancelled");

    println!("infinite loop fired_count={fired}");
    assert_eq!(fired, 3);
}

#[test]
fn currently_playing_macro_is_detected_before_delete_or_rename() {
    let state = Arc::new(AppState::new(HotkeyConfig::default()));
    state
        .set_playing_macro(Some("macro-a".to_string()))
        .expect("playing macro lock");

    println!("current playing macro={:?}", state.current_playing_macro());
    assert!(is_playing_macro(&state, "macro-a"));
    assert!(!is_playing_macro(&state, "macro-b"));
}

#[test]
fn playback_options_keep_checkbox_as_only_infinite_loop_control() {
    let sanitized = PlaybackOptions {
        loop_count: 0,
        speed_multiplier: 50.0,
        infinite_loop: false,
    }
    .sanitized();
    let infinite = PlaybackOptions {
        loop_count: 0,
        speed_multiplier: 1.0,
        infinite_loop: true,
    }
    .sanitized();

    println!("sanitized playback options={sanitized:?}; infinite={infinite:?}");
    assert_eq!(sanitized.loop_count, 1);
    assert_eq!(sanitized.speed_multiplier, 4.0);
    assert!(!sanitized.infinite_loop);
    assert_eq!(infinite.loop_count, 1);
    assert!(infinite.infinite_loop);
}

#[test]
fn macro_save_preparation_preserves_trailing_wait_as_delay_command() {
    let macro_file = MacroFile {
        name: "needs trailing delay".to_string(),
        created_at: "2026-05-15T12:00:00.000Z".to_string(),
        duration_ms: 250.0,
        events: vec![
            MacroEvent::KeyDown {
                vk_code: 0x41,
                elapsed_ms: 100.0,
            },
            MacroEvent::KeyUp {
                vk_code: 0x41,
                elapsed_ms: 50.0,
            },
        ],
    };

    let prepared = prepare_macro_for_save(macro_file).expect("macro should prepare");
    println!("prepared macro with trailing delay={prepared:?}");

    assert_eq!(prepared.duration_ms, 250.0);
    assert!(matches!(
        prepared.events[0],
        MacroEvent::Delay {
            duration_ms: 100.0,
            elapsed_ms: 100.0
        }
    ));
    assert_eq!(prepared.events[1].elapsed_ms(), 100.0);
    assert_eq!(prepared.events[2].elapsed_ms(), 100.0);
    assert!(matches!(
        prepared.events[3],
        MacroEvent::Delay {
            duration_ms: 150.0,
            elapsed_ms: 250.0
        }
    ));
}

#[test]
fn macro_save_preparation_rejects_non_finite_delay_values() {
    let macro_file = MacroFile {
        name: "bad delay".to_string(),
        created_at: "2026-05-15T12:00:00.000Z".to_string(),
        duration_ms: 0.0,
        events: vec![MacroEvent::Delay {
            duration_ms: f64::NAN,
            elapsed_ms: 0.0,
        }],
    };

    let error = prepare_macro_for_save(macro_file).expect_err("NaN delay should be rejected");
    println!("invalid delay error={error}");
    assert!(error.contains("Delay duration"));
}

#[test]
fn macro_save_preparation_merges_adjacent_delays_and_removes_zero_delays() {
    let macro_file = MacroFile {
        name: "delay clutter".to_string(),
        created_at: "2026-05-16T12:00:00.000Z".to_string(),
        duration_ms: 12.0,
        events: vec![
            MacroEvent::Delay {
                duration_ms: 10.0,
                elapsed_ms: 10.0,
            },
            MacroEvent::Delay {
                duration_ms: 0.0,
                elapsed_ms: 10.0,
            },
            MacroEvent::Delay {
                duration_ms: 2.0,
                elapsed_ms: 12.0,
            },
            MacroEvent::KeyDown {
                vk_code: 0x41,
                elapsed_ms: 12.0,
            },
        ],
    };

    let prepared = prepare_macro_for_save(macro_file).expect("macro should prepare");

    println!("prepared delay clutter macro={prepared:?}");
    assert_eq!(
        prepared.events,
        vec![
            MacroEvent::Delay {
                duration_ms: 12.0,
                elapsed_ms: 12.0
            },
            MacroEvent::KeyDown {
                vk_code: 0x41,
                elapsed_ms: 12.0
            }
        ]
    );
}

#[test]
fn new_recording_within_50ms_resets_elapsed_timestamp() {
    let state = Arc::new(AppState::new(HotkeyConfig::default()));
    state.recording.store(true, Ordering::Release);

    reset_recording_clock_for_test(&state);
    thread::sleep(Duration::from_millis(20));
    record_keyboard_event_for_test(&state, 0x41);
    let first_elapsed = first_event_elapsed(&state);

    state.events_buf.lock().expect("events buffer").clear();
    state.recording.store(false, Ordering::Release);
    thread::sleep(Duration::from_millis(10));
    state.recording.store(true, Ordering::Release);
    reset_recording_clock_for_test(&state);
    record_keyboard_event_for_test(&state, 0x42);
    let second_elapsed = first_event_elapsed(&state);

    println!("recording restart elapsed_ms: first={first_elapsed:.6}, second={second_elapsed:.6}");
    assert!(
        first_elapsed >= 10.0,
        "first recording should have non-zero elapsed time"
    );
    assert!(
        second_elapsed <= 5.0,
        "new recording should reset elapsed_ms near zero"
    );
}

#[cfg(target_os = "windows")]
fn run_scaled_playback(speed_multiplier: f64) -> Vec<f64> {
    let (control, _active) = test_control();
    let events = vec![
        MacroEvent::KeyDown {
            vk_code: 0x41,
            elapsed_ms: 20.0,
        },
        MacroEvent::KeyUp {
            vk_code: 0x41,
            elapsed_ms: 40.0,
        },
    ];
    let mut fired_ms = Vec::new();

    playback_events_for_test(
        &events,
        PlaybackOptions {
            loop_count: 1,
            speed_multiplier,
            infinite_loop: false,
        },
        &control,
        |_, _, start, fired| {
            fired_ms.push(fired.duration_since(start).as_secs_f64() * 1000.0);
            Ok(())
        },
    )
    .expect("scaled playback should complete");

    fired_ms
}

#[cfg(target_os = "windows")]
fn assert_scaled(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        let error = (actual - expected).abs();
        assert!(
            error <= 2.0,
            "scaled event fired at {actual:.6}ms, expected {expected:.6}ms"
        );
    }
}

fn first_event_elapsed(state: &AppState) -> f64 {
    let events = state.events_buf.lock().expect("events buffer");
    match events.first().expect("one recorded event") {
        MacroEvent::KeyDown { elapsed_ms, .. } => *elapsed_ms,
        event => panic!("expected keydown, got {event:?}"),
    }
}

fn test_control() -> (RunControl, Arc<AtomicBool>) {
    let state = Arc::new(AppState::new(HotkeyConfig::default()));
    let active = Arc::new(AtomicBool::new(true));
    let generation = state.run_gen.load(Ordering::Acquire);
    (RunControl::new(state, generation, active.clone()), active)
}
