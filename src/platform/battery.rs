pub struct BatteryStatus {
    pub on_battery: bool,
    pub percent: u8,
}

/// Query the current battery status. Returns None on non-macOS or desktop (no battery).
pub fn query() -> Option<BatteryStatus> {
    #[cfg(target_os = "macos")]
    return macos::query();
    #[cfg(not(target_os = "macos"))]
    return None;
}

#[cfg(target_os = "macos")]
mod macos {
    use super::BatteryStatus;
    use std::ffi::{c_char, c_int, c_void, CStr, CString};

    const UTF8: u32 = 0x08000100;
    const SINT32: c_int = 3;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPSCopyPowerSourcesInfo() -> *mut c_void;
        fn IOPSCopyPowerSourcesList(blob: *mut c_void) -> *mut c_void;
        fn IOPSGetPowerSourceDescription(blob: *mut c_void, ps: *const c_void) -> *const c_void;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(array: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(array: *const c_void, idx: isize) -> *const c_void;
        fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
        fn CFStringCreateWithCString(alloc: *const c_void, s: *const c_char, enc: u32)
            -> *mut c_void;
        fn CFStringGetCString(
            s: *const c_void,
            buf: *mut c_char,
            len: isize,
            enc: u32,
        ) -> bool;
        fn CFNumberGetValue(n: *const c_void, t: c_int, out: *mut c_void) -> bool;
        fn CFRelease(cf: *const c_void);
    }

    unsafe fn cfdict_str(dict: *const c_void, key: &str) -> Option<String> {
        let ckey = CString::new(key).ok()?;
        let cfkey = CFStringCreateWithCString(std::ptr::null(), ckey.as_ptr(), UTF8);
        if cfkey.is_null() {
            return None;
        }
        let val = CFDictionaryGetValue(dict, cfkey as *const c_void);
        CFRelease(cfkey as *const c_void);
        if val.is_null() {
            return None;
        }
        let mut buf = [0i8; 128];
        if CFStringGetCString(val, buf.as_mut_ptr(), 128, UTF8) {
            Some(CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
        } else {
            None
        }
    }

    unsafe fn cfdict_i32(dict: *const c_void, key: &str) -> Option<i32> {
        let ckey = CString::new(key).ok()?;
        let cfkey = CFStringCreateWithCString(std::ptr::null(), ckey.as_ptr(), UTF8);
        if cfkey.is_null() {
            return None;
        }
        let val = CFDictionaryGetValue(dict, cfkey as *const c_void);
        CFRelease(cfkey as *const c_void);
        if val.is_null() {
            return None;
        }
        let mut n: i32 = 0;
        if CFNumberGetValue(val, SINT32, &mut n as *mut i32 as *mut c_void) {
            Some(n)
        } else {
            None
        }
    }

    pub fn query() -> Option<BatteryStatus> {
        unsafe {
            let blob = IOPSCopyPowerSourcesInfo();
            if blob.is_null() {
                return None;
            }
            let list = IOPSCopyPowerSourcesList(blob);
            if list.is_null() {
                CFRelease(blob);
                return None;
            }
            let result = if CFArrayGetCount(list) > 0 {
                let ps = CFArrayGetValueAtIndex(list, 0);
                let desc = IOPSGetPowerSourceDescription(blob, ps);
                if !desc.is_null() {
                    let on_battery =
                        cfdict_str(desc, "Power Source State").as_deref() == Some("Battery Power");
                    let current = cfdict_i32(desc, "Current Capacity").unwrap_or(0);
                    let max = cfdict_i32(desc, "Max Capacity").unwrap_or(100).max(1);
                    let percent =
                        ((current as f32 / max as f32) * 100.0).clamp(0.0, 100.0) as u8;
                    Some(BatteryStatus { on_battery, percent })
                } else {
                    None
                }
            } else {
                None
            };
            CFRelease(list);
            CFRelease(blob);
            result
        }
    }
}
