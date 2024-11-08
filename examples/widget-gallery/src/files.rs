use floem::{
    action::{open_file, save_as},
    file::{FileDialogOptions, FileInfo, FileSpec},
    reactive::{create_rw_signal, SignalGet, SignalUpdate},
    views::{button, h_stack, label, v_stack, Decorators},
    IntoView,
};

use crate::form::form_item;

pub fn files_view() -> impl IntoView {
    let files = create_rw_signal("".to_string());
    let view = h_stack((
        button("Select file").on_click_cont(move |_| {
            open_file(
                FileDialogOptions::new()
                    .force_starting_directory("/")
                    .title("Select file")
                    .allowed_types(vec![FileSpec {
                        name: "text",
                        extensions: &["txt", "rs", "md"],
                    }]),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Selected file: {:?}", file.path);
                        files.set(display_files(file));
                    }
                },
            );
        }),
        button("Select multiple files").on_click_cont(move |_| {
            open_file(
                FileDialogOptions::new()
                    .multi_selection()
                    .title("Select file")
                    .allowed_types(vec![FileSpec {
                        name: "text",
                        extensions: &["txt", "rs", "md"],
                    }]),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Selected file: {:?}", file.path);
                        files.set(display_files(file));
                    }
                },
            );
        }),
        button("Select folder").on_click_cont(move |_| {
            open_file(
                FileDialogOptions::new()
                    .select_directories()
                    .title("Select Folder"),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Selected folder: {:?}", file.path);
                        files.set(display_files(file));
                    }
                },
            );
        }),
        button("Select multiple folder").on_click_cont(move |_| {
            open_file(
                FileDialogOptions::new()
                    .select_directories()
                    .multi_selection()
                    .title("Select multiple Folder"),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Selected folder: {:?}", file.path);
                        files.set(display_files(file));
                    }
                },
            );
        }),
        button("Save file").on_click_cont(move |_| {
            save_as(
                FileDialogOptions::new()
                    .default_name("floem.file")
                    .title("Save file"),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Save file to: {:?}", file.path);
                        files.set(display_files(file));
                    }
                },
            );
        }),
    ))
    .style(|s| {
        s.row_gap(5)
            .width_full()
            .height_full()
            .items_center()
            .justify_center()
    });

    v_stack((
        view,
        form_item("Files:".to_string(), 40.0, move || {
            label(move || files.get())
        }),
    ))
}

fn display_files(file: FileInfo) -> String {
    let paths: Vec<&str> = file
        .path
        .iter()
        .map(|p| p.to_str())
        .filter(|p| p.is_some())
        .map(|p| p.unwrap())
        .collect();
    paths.join("\n")
}
