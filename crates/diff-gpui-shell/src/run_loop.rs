use gpui::{App, QuitMode};
#[cfg(target_os = "macos")]
use objc2::MainThreadMarker;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
#[cfg(target_os = "macos")]
use objc2_foundation::NSPoint;

pub fn install(cx: &mut App) {
    cx.set_quit_mode(QuitMode::Explicit);
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            stop(cx);
        }
    })
    .detach();
}

/// Stops the run loop so that `run` returns and app state is dropped normally. On macOS,
/// `NSApp terminate:` would `exit()` without running destructors, so owned resources such as
/// child processes would never shut down.
#[cfg(target_os = "macos")]
pub fn stop(cx: &mut App) {
    let Some(main_thread) = MainThreadMarker::new() else {
        return cx.quit();
    };
    cx.hide();
    let app = NSApplication::sharedApplication(main_thread);
    app.stop(None);
    // `stop` only takes effect after the next event is processed, so post one to wake the loop.
    if let Some(wake) =
        NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
            NSEventType::ApplicationDefined,
            NSPoint::ZERO,
            NSEventModifierFlags::empty(),
            0.0,
            0,
            None,
            0,
            0,
            0,
        )
    {
        app.postEvent_atStart(&wake, false);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn stop(cx: &mut App) {
    cx.quit();
}
