use std::time::Duration;

use glazier::{
    kurbo::{Point, Vec2},
    FileDialogOptions, FileInfo,
};

use crate::{
    app_handle::{get_current_view, UpdateMessage, UPDATE_MESSAGES},
    menu::Menu,
};

fn add_update_message(msg: UpdateMessage) {
    let current_view = get_current_view();
    UPDATE_MESSAGES.with(|msgs| {
        let mut msgs = msgs.borrow_mut();
        let msgs = msgs.entry(current_view).or_default();
        msgs.push(msg);
    });
}

pub fn toggle_window_maximized() {
    add_update_message(UpdateMessage::ToggleWindowMaximized);
}

pub fn set_handle_titlebar(val: bool) {
    add_update_message(UpdateMessage::HandleTitleBar(val));
}

pub fn set_window_delta(delta: Vec2) {
    add_update_message(UpdateMessage::SetWindowDelta(delta));
}

pub fn update_window_scale(window_scale: f64) {
    add_update_message(UpdateMessage::WindowScale(window_scale));
}

pub fn exec_after(deadline: Duration, action: impl FnOnce() + 'static) {
    add_update_message(UpdateMessage::RequestTimer {
        deadline,
        action: Box::new(action),
    });
}

pub fn open_file(
    options: FileDialogOptions,
    file_info_action: impl Fn(Option<FileInfo>) + 'static,
) {
    add_update_message(UpdateMessage::OpenFile {
        options,
        file_info_action: Box::new(file_info_action),
    });
}

pub fn save_as(options: FileDialogOptions, file_info_action: impl Fn(Option<FileInfo>) + 'static) {
    add_update_message(UpdateMessage::SaveAs {
        options,
        file_info_action: Box::new(file_info_action),
    });
}

pub fn show_context_menu(menu: Menu, pos: Point) {
    add_update_message(UpdateMessage::ShowContextMenu { menu, pos });
}

pub fn set_window_menu(menu: Menu) {
    add_update_message(UpdateMessage::WindowMenu { menu });
}

pub fn set_window_title(title: String) {
    add_update_message(UpdateMessage::SetWindowTitle { title });
}
