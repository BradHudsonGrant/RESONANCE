use std::path::PathBuf;

/// Where the app keeps its database when it is not told otherwise: the platform's folder for
/// data that stays on this machine. The command line and the desktop shell both use it, so they
/// open the same library.
pub fn default_data_folder() -> Option<PathBuf> {
    let folders = directories::ProjectDirs::from("", "", "Resonance")?;
    Some(folders.data_local_dir().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_an_absolute_folder_named_for_the_app() {
        let folder = default_data_folder().expect("this machine has a home folder");

        assert!(folder.is_absolute());
        assert!(
            folder
                .to_string_lossy()
                .to_lowercase()
                .contains("resonance")
        );
    }
}
