use floem::{
    action::{open_file, save_as},
    file::{FileDialogOptions, FileSpec},
    keyboard::{Key, Modifiers, NamedKey},
    views::{button, h_stack, Decorators},
    IntoView, View,
};

fn app_view() -> impl IntoView {
    let view = h_stack((
        button(|| "Select file").on_click_cont(|_| {
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
                    }
                },
            );
        }),
        button(|| "Select multiple files").on_click_cont(|_| {
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
                    }
                },
            );
        }),
        button(|| "Select folder").on_click_cont(|_| {
            open_file(
                FileDialogOptions::new()
                    .select_directories()
                    .title("Select Folder"),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Selected folder: {:?}", file.path);
                    }
                },
            );
        }),
        button(|| "Select multiple folder").on_click_cont(|_| {
            open_file(
                FileDialogOptions::new()
                    .select_directories()
                    .multi_selection()
                    .title("Select multiple Folder"),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Selected folder: {:?}", file.path);
                    }
                },
            );
        }),
        button(|| "Save file").on_click_cont(|_| {
            save_as(
                FileDialogOptions::new()
                    .default_name("floem.file")
                    .title("Save file"),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Save file to: {:?}", file.path);
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

    let id = view.id();
    view.on_key_up(Key::Named(NamedKey::F11), Modifiers::empty(), move |_| {
        id.inspect()
    })
}

fn main() {
    floem::launch(app_view);
}
