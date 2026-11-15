fn main() -> Result<(), Box<dyn std::error::Error>> {
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "dispatch",
            "pick_audio_file",
            "pick_folder",
        ]));
    tauri_build::try_build(attributes).map_err(Into::into)
}
