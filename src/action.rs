use std::{
    path::PathBuf,
    sync::atomic::AtomicU64,
    time::{Duration, Instant},
};

use floem_reactive::Scope;
use floem_winit::window::ResizeDirection;
use kurbo::{Point, Size, Vec2};

use crate::{
    app::{add_app_update_event, AppUpdateEvent},
    ext_event::create_ext_action,
    file::{FileDialogOptions, FileInfo},
    id::Id,
    menu::Menu,
    update::{UpdateMessage, CENTRAL_UPDATE_MESSAGES},
    view::Widget,
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
    let send = create_ext_action(
        Scope::new(),
        move |(path, paths): (Option<PathBuf>, Option<Vec<PathBuf>>)| {
            if paths.is_some() {
                file_info_action(paths.map(|paths| FileInfo {
                    path: paths,
                    format: None,
                }))
            } else {
                file_info_action(path.map(|path| FileInfo {
                    path: vec![path],
                    format: None,
                }))
            }
        },
    );
    std::thread::spawn(move || {
        let mut dialog = rfd::FileDialog::new();
        if let Some(path) = options.starting_directory.as_ref() {
            dialog = dialog.set_directory(path);
        }
        if let Some(title) = options.title.as_ref() {
            dialog = dialog.set_title(title);
        }
        if let Some(allowed_types) = options.allowed_types.as_ref() {
            dialog = allowed_types.iter().fold(dialog, |dialog, filter| {
                dialog.add_filter(filter.name, filter.extensions)
            });
        }

        if options.select_directories && options.multi_selection {
            send((None, dialog.pick_folders()));
        } else if options.select_directories && !options.multi_selection {
            send((dialog.pick_folder(), None));
        } else if !options.select_directories && options.multi_selection {
            send((None, dialog.pick_files()));
        } else {
            send((dialog.pick_file(), None));
        }
    });
}

pub fn save_as(options: FileDialogOptions, file_info_action: impl Fn(Option<FileInfo>) + 'static) {
    let send = create_ext_action(Scope::new(), move |path: Option<PathBuf>| {
        file_info_action(path.map(|path| FileInfo {
            path: vec![path],
            format: None,
        }))
    });
    std::thread::spawn(move || {
        let mut dialog = rfd::FileDialog::new();
        if let Some(path) = options.starting_directory.as_ref() {
            dialog = dialog.set_directory(path);
        }
        if let Some(name) = options.default_name.as_ref() {
            dialog = dialog.set_file_name(name);
        }
        if let Some(title) = options.title.as_ref() {
            dialog = dialog.set_title(title);
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

pub fn focus_window() {
    add_update_message(UpdateMessage::FocusWindow);
}

pub fn set_ime_allowed(allowed: bool) {
    add_update_message(UpdateMessage::SetImeAllowed { allowed });
}

pub fn set_ime_cursor_area(position: Point, size: Size) {
    add_update_message(UpdateMessage::SetImeCursorArea { position, size });
}

/// Creates a new overlay on the current window.
pub fn add_overlay<V: Widget + 'static>(
    position: Point,
    view: impl FnOnce(Id) -> V + 'static,
) -> Id {
    let id = Id::next();
    add_update_message(UpdateMessage::AddOverlay {
        id,
        position,
        view: Box::new(move || Box::new(view(id))),
    });
    id
}

/// Removes an overlay from the current window.
pub fn remove_overlay(id: Id) {
    add_update_message(UpdateMessage::RemoveOverlay { id });
}
