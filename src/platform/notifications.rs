/// Deliver a native notification with the given body text.
/// No-op on non-macOS or if the app has not been granted notification permission.
pub fn send(body: &str) {
    #[cfg(target_os = "macos")]
    macos::send(body);
    #[cfg(not(target_os = "macos"))]
    let _ = body;
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

    pub(super) fn send(body: &str) {
        unsafe {
            let center = UNUserNotificationCenter::currentNotificationCenter();

            // Request permission on first call; no-op if already decided.
            let opts = UNAuthorizationOptions::UNAuthorizationOptionAlert
                | UNAuthorizationOptions::UNAuthorizationOptionSound;
            let noop = RcBlock::new(|_: Bool, _: *mut NSError| {});
            center.requestAuthorizationWithOptions_completionHandler(opts, &noop);

            let content = UNMutableNotificationContent::new();
            content.setTitle(&NSString::from_str("PetruTerm"));
            content.setBody(&NSString::from_str(body));

            // Reuse a fixed identifier so rapid notifications replace each other.
            let req = UNNotificationRequest::requestWithIdentifier_content_trigger(
                &NSString::from_str("petruterm.toast"),
                &content,
                None,
            );
            center.addNotificationRequest_withCompletionHandler(&req, None);
        }
    }
}
