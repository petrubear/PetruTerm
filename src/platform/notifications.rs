/// Deliver a native notification with the given body text.
/// No-op on non-macOS or if the app has not been granted notification permission.
pub fn send(body: &str) {
    send_with("PetruTerm", body, "petruterm.toast");
}

/// Deliver a native notification with an explicit title and identifier. Notifications that
/// share an identifier replace each other, so per-pane callers pass a per-pane identifier.
pub fn send_with(title: &str, body: &str, identifier: &str) {
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
