use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileSpec {
    /// A human readable name, describing this filetype.
    ///
    /// This is used in the Windows file dialog, where the user can select
    /// from a dropdown the type of file they would like to choose.
    ///
    /// This should not include the file extensions; they will be added automatically.
    /// For instance, if we are describing Word documents, the name would be "Word Document",
    /// and the displayed string would be "Word Document (*.doc)".
    pub name: &'static str,
    /// The file extensions used by this file type.
    ///
    /// This should not include the leading '.'.
    pub extensions: &'static [&'static str],
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    /// The path to the selected file.
    ///
    /// On macOS, this is already rewritten to use the extension that the user selected
    /// with the `file format` property.
    pub path: PathBuf,
    /// The selected file format.
    ///
    /// If there're multiple different formats available
    /// this allows understanding the kind of format that the user expects the file
    /// to be written in. Examples could be Blender 2.4 vs Blender 2.6 vs Blender 2.8.
    /// The `path` above will already contain the appropriate extension chosen in the
    /// `format` property, so it is not necessary to mutate `path` any further.
    pub format: Option<FileSpec>,
}

impl FileInfo {
    /// Returns the underlying path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[derive(Debug, Clone, Default)]
pub struct FileDialogOptions {
    pub(crate) show_hidden: bool,
    pub(crate) allowed_types: Option<Vec<FileSpec>>,
    pub(crate) default_type: Option<FileSpec>,
    pub(crate) select_directories: bool,
    pub(crate) packages_as_directories: bool,
    pub(crate) multi_selection: bool,
    pub(crate) default_name: Option<String>,
    pub(crate) name_label: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) button_text: Option<String>,
    pub(crate) starting_directory: Option<PathBuf>,
}

impl FileDialogOptions {
    /// Create a new set of options.
    pub fn new() -> FileDialogOptions {
        FileDialogOptions::default()
    }

    /// Set hidden files and directories to be visible.
    pub fn show_hidden(mut self) -> Self {
        self.show_hidden = true;
        self
    }

    /// Set directories to be selectable instead of files.
    ///
    /// This is only relevant for open dialogs.
    pub fn select_directories(mut self) -> Self {
        self.select_directories = true;
        self
    }

    /// Set [packages] to be treated as directories instead of files.
    ///
    /// This allows for writing more universal cross-platform code at the cost of user experience.
    ///
    /// This is only relevant on macOS.
    ///
    /// [packages]: #packages
    pub fn packages_as_directories(mut self) -> Self {
        self.packages_as_directories = true;
        self
    }

    /// Set multiple items to be selectable.
    ///
    /// This is only relevant for open dialogs.
    pub fn multi_selection(mut self) -> Self {
        self.multi_selection = true;
        self
    }

    /// Set the file types the user is allowed to select.
    ///
    /// This filter is only applied to files and [packages], but not to directories.
    ///
    /// An empty collection is treated as no filter.
    ///
    /// # macOS
    ///
    /// These file types also apply to directories to define [packages].
    /// Which means the directories that match the filter are no longer considered directories.
    /// The packages are defined by this collection even in *directories mode*.
    ///
    /// [packages]: #packages
    pub fn allowed_types(mut self, types: Vec<FileSpec>) -> Self {
        // An empty vector can cause platform issues, so treat it as no filter
        if types.is_empty() {
            self.allowed_types = None;
        } else {
            self.allowed_types = Some(types);
        }
        self
    }

    /// Set the default file type.
    ///
    /// The provided `default_type` must also be present in [`allowed_types`].
    ///
    /// If it's `None` then the first entry in [`allowed_types`] will be used as the default.
    ///
    /// This is only relevant in *files mode*.
    ///
    /// [`allowed_types`]: #method.allowed_types
    pub fn default_type(mut self, default_type: FileSpec) -> Self {
        self.default_type = Some(default_type);
        self
    }

    /// Set the default filename that appears in the dialog.
    pub fn default_name(mut self, default_name: impl Into<String>) -> Self {
        self.default_name = Some(default_name.into());
        self
    }

    /// Set the text in the label next to the filename editbox.
    pub fn name_label(mut self, name_label: impl Into<String>) -> Self {
        self.name_label = Some(name_label.into());
        self
    }

    /// Set the title text of the dialog.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the text of the Open/Save button.
    pub fn button_text(mut self, text: impl Into<String>) -> Self {
        self.button_text = Some(text.into());
        self
    }

    /// Force the starting directory to the specified `path`.
    ///
    /// # User experience
    ///
    /// This should almost never be used because it overrides the OS choice,
    /// which will usually be a directory that the user recently visited.
    pub fn force_starting_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.starting_directory = Some(path.into());
        self
    }
}
