/// Deliver a native notification with the given body text.
/// No-op on non-macOS or if the app has not been granted notification permission.
pub fn send(body: &str) {
    send_with("PetruTerm", body, "petruterm.toast");
}

/// Whether native notifications can be delivered. `UNUserNotificationCenter` raises an
/// uncaught `NSException` (SIGABRT) when the process is not running from an `.app` bundle
/// (e.g. `cargo run`), so every native send is gated on this. Callers should fall back to an
/// in-window toast when it is false.
pub fn native_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        static BUNDLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *BUNDLED.get_or_init(|| {
            std::env::current_exe().ok().is_some_and(|exe| {
                exe.ancestors()
                    .any(|a| a.extension().is_some_and(|e| e == "app"))
            })
        })
    }
    #[cfg(not(target_os = "macos"))]
    false
}

/// Deliver a native notification with an explicit title and identifier. Notifications that
/// share an identifier replace each other, so per-pane callers pass a per-pane identifier.
/// No-op when [`native_available`] is false.
pub fn send_with(title: &str, body: &str, identifier: &str) {
    if !native_available() {
        return;
    }
    #[cfg(target_os = "macos")]
    macos::send(title, body, identifier);
    #[cfg(not(target_os = "macos"))]
    let _ = (title, body, identifier);
}

#[cfg(target_os = "macos")]
mod macos {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
        UNUserNotificationCenter,
    };

    pub(super) fn send(title: &str, body: &str, identifier: &str) {
        unsafe {
            let center = UNUserNotificationCenter::currentNotificationCenter();

            // Request permission on first call; no-op if already decided.
            let opts = UNAuthorizationOptions::UNAuthorizationOptionAlert
                | UNAuthorizationOptions::UNAuthorizationOptionSound;
            let noop = RcBlock::new(|_: Bool, _: *mut NSError| {});
            center.requestAuthorizationWithOptions_completionHandler(opts, &noop);

            let content = UNMutableNotificationContent::new();
            content.setTitle(&NSString::from_str(title));
            content.setBody(&NSString::from_str(body));

            // Same identifier => the newer notification replaces the older one.
            let req = UNNotificationRequest::requestWithIdentifier_content_trigger(
                &NSString::from_str(identifier),
                &content,
                None,
            );
            center.addNotificationRequest_withCompletionHandler(&req, None);
        }
    }
}

#[cfg(test)]
mod tests {
    /// Regression: the test binary is not an `.app` bundle, and calling
    /// `UNUserNotificationCenter` from one aborts the process. This must be a silent no-op.
    #[test]
    fn send_from_unbundled_process_does_not_abort() {
        assert!(!super::native_available());
        super::send_with("title", "body", "petruterm.test");
        super::send("body");
    }
}
