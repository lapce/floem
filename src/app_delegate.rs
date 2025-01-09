use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{declare_class, msg_send_id, mutability, ClassType, DeclaredClass};
use objc2_app_kit::{NSApplication, NSApplicationDelegate};
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol};

use crate::app::UserEvent;

declare_class!(
    struct AppDelegate;

    unsafe impl ClassType for AppDelegate {
        type Super = NSObject;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "MyAppDelegate";
    }

    impl DeclaredClass for AppDelegate {}

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[method(applicationShouldHandleReopen:hasVisibleWindows:)]
        fn should_handle_reopen(&self, _sender: &Option<&AnyObject>, has_visible_windows: bool) -> bool {
            crate::Application::send_proxy_event(UserEvent::Reopen { has_visible_windows });
            // return true to preserve the default behavior, such as showing the minimized window.
            true
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send_id![super(mtm.alloc().set_ivars(())), init] }
    }
}

pub(crate) fn set_app_delegate() {
    let mtm = MainThreadMarker::new().unwrap();
    let delegate = AppDelegate::new(mtm);
    // Important: Call `sharedApplication` after `EventLoop::new`,
    // doing it before is not yet supported.
    let app = NSApplication::sharedApplication(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
}
