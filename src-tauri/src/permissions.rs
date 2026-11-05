//! System permissions: status, one-time request, and a deep link to the right
//! settings pane. Which permissions matter is configured in
//! `starter.config.json` → `permissions.{required,optional}`.
//!
//! macOS checks are pure Rust FFI into the system frameworks (no Swift bridge),
//! so the template builds unchanged on Windows and Linux, where most of these
//! are simply `NotApplicable`.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Notifications,
    Accessibility,
    ScreenRecording,
    Microphone,
    Camera,
    InputMonitoring,
}

impl Permission {
    pub const ALL: [Permission; 6] = [
        Permission::Notifications,
        Permission::Accessibility,
        Permission::ScreenRecording,
        Permission::Microphone,
        Permission::Camera,
        Permission::InputMonitoring,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Granted,
    Denied,
    /// The system has not asked the user yet — `request` will show the prompt.
    Undetermined,
    /// This platform has no such permission; treat as granted.
    NotApplicable,
}

impl PermissionState {
    pub fn is_satisfied(self) -> bool {
        matches!(self, Self::Granted | Self::NotApplicable)
    }
}

/// One row of the health check. Mirrored by `PermissionReport` in `src/lib/ipc.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PermissionReport {
    pub permission: Permission,
    pub state: PermissionState,
    pub required: bool,
    /// Whether `request` can show a system prompt (false once denied on macOS,
    /// where only System Settings can flip it back).
    pub can_request: bool,
    /// Whether `open_settings` knows a deep link on this platform.
    pub can_open_settings: bool,
}

/// Status for every configured permission (required first).
pub fn report(app: &AppHandle) -> Vec<PermissionReport> {
    let cfg = &crate::config::get().permissions;
    cfg.required
        .iter()
        .map(|p| (*p, true))
        .chain(
            cfg.optional
                .iter()
                .filter(|p| !cfg.required.contains(p))
                .map(|p| (*p, false)),
        )
        .map(|(permission, required)| {
            let state = status(app, permission);
            PermissionReport {
                permission,
                state,
                required,
                can_request: state == PermissionState::Undetermined
                    || (state == PermissionState::Denied
                        && platform::request_always_prompts(permission)),
                can_open_settings: platform::settings_url(permission).is_some(),
            }
        })
        .collect()
}

pub fn missing_required(reports: &[PermissionReport]) -> Vec<Permission> {
    reports
        .iter()
        .filter(|r| r.required && !r.state.is_satisfied())
        .map(|r| r.permission)
        .collect()
}

pub fn status(app: &AppHandle, p: Permission) -> PermissionState {
    match p {
        Permission::Notifications => notifications_status(app),
        other => platform::status(other),
    }
}

/// Fire the system prompt (where one exists) and return the best-known state
/// afterwards. The UI should re-poll `status` regardless: some prompts resolve
/// asynchronously.
pub fn request(app: &AppHandle, p: Permission) -> PermissionState {
    match p {
        Permission::Notifications => {
            use tauri_plugin_notification::NotificationExt;
            let _ = app.notification().request_permission();
            notifications_status(app)
        }
        other => platform::request(other),
    }
}

/// Open the OS settings pane for `p`. Returns false when there is no deep link.
pub fn open_settings(app: &AppHandle, p: Permission) -> bool {
    use tauri_plugin_opener::OpenerExt;
    match platform::settings_url(p) {
        Some(url) => app.opener().open_url(url, None::<&str>).is_ok(),
        None => false,
    }
}

fn notifications_status(app: &AppHandle) -> PermissionState {
    use tauri_plugin_notification::{NotificationExt, PermissionState as S};
    match app.notification().permission_state() {
        Ok(S::Granted) => PermissionState::Granted,
        Ok(S::Denied) => PermissionState::Denied,
        Ok(_) => PermissionState::Undetermined,
        Err(_) => PermissionState::Undetermined,
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{Permission, PermissionState};
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};
    use objc2_av_foundation::{
        AVAuthorizationStatus, AVCaptureDevice, AVMediaType, AVMediaTypeAudio, AVMediaTypeVideo,
    };

    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
        fn IOHIDCheckAccess(request_type: u32) -> u32;
        fn IOHIDRequestAccess(request_type: u32) -> bool;
    }

    const IOHID_REQUEST_LISTEN_EVENT: u32 = 1;

    fn bool_state(granted: bool) -> PermissionState {
        if granted {
            PermissionState::Granted
        } else {
            PermissionState::Denied
        }
    }

    fn media_type(p: Permission) -> &'static AVMediaType {
        // SAFETY: framework-provided constants, valid for the process lifetime.
        unsafe {
            if p == Permission::Camera {
                AVMediaTypeVideo
            } else {
                AVMediaTypeAudio
            }
            .expect("AVFoundation media-type constant")
        }
    }

    fn media_status(p: Permission) -> PermissionState {
        // SAFETY: plain class method with a valid media-type constant.
        let s = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type(p)) };
        match s {
            AVAuthorizationStatus::Authorized => PermissionState::Granted,
            AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => {
                PermissionState::Denied
            }
            _ => PermissionState::Undetermined,
        }
    }

    pub fn status(p: Permission) -> PermissionState {
        // SAFETY: all calls are prompt-free status queries.
        unsafe {
            match p {
                Permission::Accessibility => bool_state(AXIsProcessTrusted()),
                Permission::ScreenRecording => bool_state(CGPreflightScreenCaptureAccess()),
                Permission::InputMonitoring => match IOHIDCheckAccess(IOHID_REQUEST_LISTEN_EVENT) {
                    0 => PermissionState::Granted,
                    1 => PermissionState::Denied,
                    _ => PermissionState::Undetermined,
                },
                Permission::Microphone | Permission::Camera => media_status(p),
                Permission::Notifications => PermissionState::Undetermined,
            }
        }
    }

    /// TCC only shows the Accessibility / Screen Recording / Input Monitoring
    /// prompt once per app identity; after that the user must flip the toggle
    /// in System Settings. Mic/camera prompt once too but report Undetermined
    /// until then, so they are covered by the generic rule.
    pub fn request_always_prompts(_p: Permission) -> bool {
        false
    }

    pub fn request(p: Permission) -> PermissionState {
        match p {
            Permission::Accessibility => {
                // SAFETY: builds {kAXTrustedCheckOptionPrompt: true} and passes it by ref.
                unsafe {
                    let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
                    let dict = CFDictionary::from_CFType_pairs(&[(
                        key.as_CFType(),
                        CFBoolean::true_value().as_CFType(),
                    )]);
                    bool_state(AXIsProcessTrustedWithOptions(dict.as_concrete_TypeRef()))
                }
            }
            // SAFETY: prompt-triggering framework calls with no arguments.
            Permission::ScreenRecording => unsafe { bool_state(CGRequestScreenCaptureAccess()) },
            Permission::InputMonitoring => unsafe {
                IOHIDRequestAccess(IOHID_REQUEST_LISTEN_EVENT);
                status(p)
            },
            Permission::Microphone | Permission::Camera => {
                let handler = block2::RcBlock::new(|_granted: objc2::runtime::Bool| {});
                // SAFETY: the completion block is retained by the framework.
                unsafe {
                    AVCaptureDevice::requestAccessForMediaType_completionHandler(
                        media_type(p),
                        &handler,
                    );
                }
                media_status(p)
            }
            Permission::Notifications => PermissionState::Undetermined,
        }
    }

    pub fn settings_url(p: Permission) -> Option<&'static str> {
        const P: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_";
        Some(match p {
            Permission::Notifications => {
                "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
            }
            Permission::Accessibility => concat!(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_",
                "Accessibility"
            ),
            Permission::ScreenRecording => concat!(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_",
                "ScreenCapture"
            ),
            Permission::Microphone => concat!(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_",
                "Microphone"
            ),
            Permission::Camera => concat!(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_",
                "Camera"
            ),
            Permission::InputMonitoring => concat!(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_",
                "ListenEvent"
            ),
        })
        .inspect(|u| debug_assert!(u.starts_with(P) || p == Permission::Notifications))
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{Permission, PermissionState};

    pub fn status(p: Permission) -> PermissionState {
        match p {
            // Windows gates mic/camera per app in Settings but offers no
            // reliable query for unpackaged desktop apps; assume granted and
            // let the capture call fail loudly if not.
            Permission::Microphone | Permission::Camera => PermissionState::Granted,
            _ => PermissionState::NotApplicable,
        }
    }
    pub fn request_always_prompts(_p: Permission) -> bool {
        false
    }
    pub fn request(p: Permission) -> PermissionState {
        status(p)
    }
    pub fn settings_url(p: Permission) -> Option<&'static str> {
        match p {
            Permission::Notifications => Some("ms-settings:notifications"),
            Permission::Microphone => Some("ms-settings:privacy-microphone"),
            Permission::Camera => Some("ms-settings:privacy-webcam"),
            _ => None,
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::{Permission, PermissionState};

    pub fn status(_p: Permission) -> PermissionState {
        PermissionState::NotApplicable
    }
    pub fn request_always_prompts(_p: Permission) -> bool {
        false
    }
    pub fn request(p: Permission) -> PermissionState {
        status(p)
    }
    pub fn settings_url(_p: Permission) -> Option<&'static str> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_is_snake_case() {
        assert_eq!(
            serde_json::to_string(&Permission::ScreenRecording).unwrap(),
            "\"screen_recording\""
        );
        let p: Permission = serde_json::from_str("\"input_monitoring\"").unwrap();
        assert_eq!(p, Permission::InputMonitoring);
        assert_eq!(
            serde_json::to_string(&PermissionState::NotApplicable).unwrap(),
            "\"not_applicable\""
        );
    }

    #[test]
    fn satisfied_means_granted_or_not_applicable() {
        assert!(PermissionState::Granted.is_satisfied());
        assert!(PermissionState::NotApplicable.is_satisfied());
        assert!(!PermissionState::Denied.is_satisfied());
        assert!(!PermissionState::Undetermined.is_satisfied());
    }

    #[test]
    fn missing_required_ignores_optional_and_satisfied() {
        let mk = |permission, state, required| PermissionReport {
            permission,
            state,
            required,
            can_request: false,
            can_open_settings: false,
        };
        let reports = vec![
            mk(Permission::Accessibility, PermissionState::Denied, true),
            mk(Permission::ScreenRecording, PermissionState::Granted, true),
            mk(Permission::Microphone, PermissionState::Denied, false),
            mk(Permission::Camera, PermissionState::NotApplicable, true),
        ];
        assert_eq!(missing_required(&reports), vec![Permission::Accessibility]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_settings_links_point_at_privacy_panes() {
        for p in Permission::ALL {
            let url = platform::settings_url(p).expect("every permission has a pane on macOS");
            assert!(url.starts_with("x-apple.systempreferences:"), "{url}");
        }
    }
}
