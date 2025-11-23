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
        button("Select file").action(move || {
            open_file(
                FileDialogOptions::new()
                    .force_starting_directory("/")
                    .title("Select file (txt, rs, md)")
                    .allowed_types(vec![FileSpec {
                        name: "text",
                        extensions: &["txt", "rs", "md"],
                    }]),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Selected file: {:?}", file.paths);
                        files.set(display_files(file));
                    }
                },
            );
        }),
        button("Select multiple files").action(move || {
            open_file(
                FileDialogOptions::new()
                    .multi_selection()
                    .title("Select multiple files (txt, rs, md)")
                    .allowed_types(vec![FileSpec {
                        name: "text",
                        extensions: &["txt", "rs", "md"],
                    }]),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Selected file: {:?}", file.paths);
                        files.set(display_files(file));
                    }
                },
            );
        }),
        button("Select folder").action(move || {
            open_file(
                FileDialogOptions::new()
                    .select_directories()
                    .title("Select Folder"),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Selected folder: {:?}", file.paths);
                        files.set(display_files(file));
                    }
                },
            );
        }),
        button("Select multiple folders").action(move || {
            open_file(
                FileDialogOptions::new()
                    .select_directories()
                    .multi_selection()
                    .title("Select multiple folders"),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Selected folder: {:?}", file.paths);
                        files.set(display_files(file));
                    }
                },
            );
        }),
        button("Save file").action(move || {
            save_as(
                FileDialogOptions::new()
                    .default_name("floem.file")
                    .title("Save file"),
                move |file_info| {
                    if let Some(file) = file_info {
                        println!("Save file to: {:?}", file.paths);
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
    let paths: Vec<&str> = file.paths.iter().filter_map(|p| p.to_str()).collect();
    paths.join("\n")
}
