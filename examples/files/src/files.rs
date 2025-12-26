use floem::{
    action::{open_file, save_as},
    file::{FileDialogOptions, FileInfo, FileSpec},
    reactive::{RwSignal, SignalGet, SignalUpdate},
    text::Weight,
    views::{Button, Decorators, Label, Stack},
    IntoView,
};

pub fn files_view() -> impl IntoView {
    let files = RwSignal::new(String::new());
    let view = Stack::horizontal((
        Button::new("Select file").on_click_cont(move |_| {
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
        Button::new("Select multiple files").on_click_cont(move |_| {
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
        Button::new("Select folder").on_click_cont(move |_| {
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
        Button::new("Select multiple folder").on_click_cont(move |_| {
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
        Button::new("Save file").on_click_cont(move |_| {
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

    Stack::vertical((
        view,
        Stack::horizontal((
            "Path(s): ".style(|s| s.font_weight(Weight::BOLD)),
            Label::derived(move || files.get()),
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
