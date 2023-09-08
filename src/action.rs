use std::{
    path::PathBuf,
    sync::atomic::AtomicU64,
    time::{Duration, Instant},
};

use floem_reactive::Scope;
use kurbo::{Point, Size, Vec2};
use winit::window::ResizeDirection;

use crate::{
    app::{add_app_update_event, AppUpdateEvent},
    ext_event::create_ext_action,
    file::{FileDialogOptions, FileInfo},
    menu::Menu,
    update::{UpdateMessage, CENTRAL_UPDATE_MESSAGES},
    window_handle::{get_current_view, set_current_view},
};

fn add_update_message(msg: UpdateMessage) {
    let current_view = get_current_view();
    CENTRAL_UPDATE_MESSAGES.with(|msgs| {
        msgs.borrow_mut().push((current_view, msg));
    });
}

pub fn toggle_window_maximized() {
    add_update_message(UpdateMessage::ToggleWindowMaximized);
}

pub fn set_window_maximized(maximized: bool) {
    add_update_message(UpdateMessage::SetWindowMaximized(maximized));
}

pub fn minimize_window() {
    add_update_message(UpdateMessage::MinimizeWindow);
}

pub fn drag_window() {
    add_update_message(UpdateMessage::DragWindow);
}

pub fn drag_resize_window(direction: ResizeDirection) {
    add_update_message(UpdateMessage::DragResizeWindow(direction));
}

pub fn set_window_delta(delta: Vec2) {
    add_update_message(UpdateMessage::SetWindowDelta(delta));
}

pub fn update_window_scale(window_scale: f64) {
    add_update_message(UpdateMessage::WindowScale(window_scale));
}

pub(crate) struct Timer {
    pub(crate) token: TimerToken,
    pub(crate) action: Box<dyn FnOnce(TimerToken)>,
    pub(crate) deadline: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub struct TimerToken(u64);

impl TimerToken {
    /// A token that does not correspond to any timer.
    pub const INVALID: TimerToken = TimerToken(0);

    /// Create a new token.
    pub fn next() -> TimerToken {
        static TIMER_COUNTER: AtomicU64 = AtomicU64::new(0);
        TimerToken(TIMER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    /// Create a new token from a raw value.
    pub const fn from_raw(id: u64) -> TimerToken {
        TimerToken(id)
    }

    /// Get the raw value for a token.
    pub const fn into_raw(self) -> u64 {
        self.0
    }
}

pub fn exec_after(duration: Duration, action: impl FnOnce(TimerToken) + 'static) -> TimerToken {
    let view = get_current_view();
    let action = move |token| {
        let current_view = get_current_view();
        set_current_view(view);
        action(token);
        set_current_view(current_view);
    };

    let token = TimerToken::next();
    let deadline = Instant::now() + duration;
    add_app_update_event(AppUpdateEvent::RequestTimer {
        timer: Timer {
            token,
            action: Box::new(action),
            deadline,
        },
    });
    token
}

pub fn open_file(
    options: FileDialogOptions,
    file_info_action: impl Fn(Option<FileInfo>) + 'static,
) {
    let send = create_ext_action(Scope::new(), move |path: Option<PathBuf>| {
        file_info_action(path.map(|path| FileInfo { path, format: None }))
    });
    std::thread::spawn(move || {
        let mut dialog = rfd::FileDialog::new();
        if let Some(path) = options.starting_directory.as_ref() {
            dialog = dialog.set_directory(path);
        }
        let path = if options.select_directories {
            dialog.pick_folder()
        } else {
            dialog.pick_file()
        };
        send(path);
    });
}

pub fn save_as(options: FileDialogOptions, file_info_action: impl Fn(Option<FileInfo>) + 'static) {
    let send = create_ext_action(Scope::new(), move |path: Option<PathBuf>| {
        file_info_action(path.map(|path| FileInfo { path, format: None }))
    });
    std::thread::spawn(move || {
        let mut dialog = rfd::FileDialog::new();
        if let Some(path) = options.starting_directory.as_ref() {
            dialog = dialog.set_directory(path);
        }
        let path = dialog.save_file();
        send(path);
    });
}

pub fn show_context_menu(menu: Menu, pos: Option<Point>) {
    add_update_message(UpdateMessage::ShowContextMenu { menu, pos });
}

pub fn set_window_menu(menu: Menu) {
    add_update_message(UpdateMessage::WindowMenu { menu });
}

pub fn set_window_title(title: String) {
    add_update_message(UpdateMessage::SetWindowTitle { title });
}

pub fn set_ime_allowed(allowed: bool) {
    add_update_message(UpdateMessage::SetImeAllowed { allowed });
}

pub fn set_ime_cursor_area(position: Point, size: Size) {
    add_update_message(UpdateMessage::SetImeCursorArea { position, size });
}
