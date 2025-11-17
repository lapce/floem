use std::{path::PathBuf, pin::Pin};

use floem_reactive::{Scope, SignalGet as _, create_updater, with_scope};
use futures::FutureExt;

use crate::{
    ext_event::async_signal::FutureSignal,
    file::{FileDialogOptions, FileInfo},
};

/// Open a file using the system file dialog
pub fn open_file(
    options: FileDialogOptions,
    file_info_action: impl Fn(Option<FileInfo>) + 'static,
) {
    let mut dialog = rfd::AsyncFileDialog::new();

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

    fn to_path_vec(handle: rfd::FileHandle) -> Vec<PathBuf> {
        vec![handle.path().to_path_buf()]
    }

    fn to_path_vecs(handles: Vec<rfd::FileHandle>) -> Vec<PathBuf> {
        handles.iter().map(|h| h.path().to_path_buf()).collect()
    }

    // Create the appropriate future based on options, mapping to unified return type
    let future = match (options.select_directories, options.multi_selection) {
        (true, true) => Box::pin(dialog.pick_folders().map(|opt| opt.map(to_path_vecs)))
            as Pin<Box<dyn Future<Output = Option<Vec<PathBuf>>>>>,
        (true, false) => {
            Box::pin(dialog.pick_folder().map(|opt| opt.map(to_path_vec))) as Pin<Box<_>>
        }
        (false, true) => {
            Box::pin(dialog.pick_files().map(|opt| opt.map(to_path_vecs))) as Pin<Box<_>>
        }
        (false, false) => {
            Box::pin(dialog.pick_file().map(|opt| opt.map(to_path_vec))) as Pin<Box<_>>
        }
    };

    let scope = Scope::new();
    with_scope(scope, || {
        let resource = FutureSignal::on_event_loop(future);
        create_updater(
            move || resource.get(),
            move |paths| {
                if let Some(paths) = paths {
                    if let Some(paths) = paths {
                        file_info_action(Some(FileInfo {
                            paths,
                            format: None,
                        }));
                    }
                    scope.dispose();
                }
            },
        );
    });
}

/// Open a system file save dialog
pub fn save_as(options: FileDialogOptions, file_info_action: impl Fn(Option<FileInfo>) + 'static) {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(path) = options.starting_directory.as_ref() {
        dialog = dialog.set_directory(path);
    }
    if let Some(name) = options.default_name.as_ref() {
        dialog = dialog.set_file_name(name);
    }
    if let Some(title) = options.title.as_ref() {
        dialog = dialog.set_title(title);
    }

    let future = dialog
        .save_file()
        .map(|opt| opt.map(|h| h.path().to_path_buf()));

    let scope = Scope::new();
    with_scope(scope, || {
        let resource = FutureSignal::on_event_loop(future);
        create_updater(
            move || resource.get(),
            move |path| {
                if let Some(path) = path {
                    if let Some(path) = path {
                        file_info_action(Some(FileInfo {
                            paths: vec![path],
                            format: None,
                        }));
                    }
                    scope.dispose();
                }
            },
        );
    });
}
