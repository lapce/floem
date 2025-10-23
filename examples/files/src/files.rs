use floem::{
    action::{open_file, save_as},
    file::{FileDialogOptions, FileInfo, FileSpec},
    reactive::{create_rw_signal, SignalGet, SignalUpdate},
    text::Weight,
    views::{button, h_stack, label, v_stack, Decorators},
    IntoView,
};

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
    .style(|s| s.justify_center().gap(10));

    v_stack((
        view,
        h_stack((
            "Path(s): ".style(|s| s.font_weight(Weight::BOLD)),
            label(move || files.get()),
        )),
    ))
    .style(|s| {
        s.row_gap(5)
            .padding(10)
            .width_full()
            .height_full()
            .items_center()
            .justify_center()
    })
}

fn display_files(file: FileInfo) -> String {
    let paths: Vec<&str> = file.path.iter().filter_map(|p| p.to_str()).collect();
    paths.join("\n")
}
