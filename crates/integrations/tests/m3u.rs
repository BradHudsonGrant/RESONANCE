use std::fs;
use std::path::{Path, PathBuf};

use resonance_core::playlist::{PathChange, PlaylistTrack, normalise_path};
use resonance_integrations::{ExternalLibraryAdapter, IntegrationError, m3u};

fn sandbox(test: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(test);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("Music/House")).expect("sandbox");
    fs::create_dir_all(root.join("Sets")).expect("sandbox");
    root
}

fn text(path: &Path) -> String {
    normalise_path(&path.to_string_lossy())
}

#[test]
fn reads_relative_and_absolute_entries_and_skips_streams() {
    let root = sandbox("m3u-read");
    let playlist = root.join("Sets/Friday.m3u8");
    let absolute = text(&root.join("Music/House/b.flac")).replace('/', "\\");
    fs::write(
        &playlist,
        format!(
            "#EXTM3U\n#EXTINF:301,Ada Marsh - First Light\n../Music/House/a.mp3\n{absolute}\nhttp://radio.example/live\n"
        ),
    )
    .expect("playlist");

    let library = m3u::M3u.read_library(&playlist).expect("read");
    let friday = &library.playlists[0];
    assert_eq!(friday.name, "Friday");
    assert_eq!(friday.skipped, 1);
    assert_eq!(friday.entries.len(), 2);
    assert_eq!(
        friday.entries[0].path,
        text(&root.join("Music/House/a.mp3"))
    );
    assert_eq!(friday.entries[0].duration_s, Some(301.0));
    assert_eq!(
        friday.entries[0].title.as_deref(),
        Some("Ada Marsh - First Light")
    );
    assert_eq!(
        friday.entries[1].path,
        text(&root.join("Music/House/b.flac"))
    );

    fs::write(root.join("Music/House/notes.txt"), "not a playlist").expect("note");
    fs::write(root.join("Music/Old.m3u"), "a.mp3\n").expect("second playlist");
    assert_eq!(
        m3u::M3u.detect(&root).expect("detect"),
        [root.join("Music/Old.m3u"), playlist]
    );
}

#[test]
fn a_dry_run_writes_nothing_and_apply_backs_up_then_rewrites() {
    let root = sandbox("m3u-apply");
    let playlist = root.join("Sets/Friday.m3u8");
    let original = "#EXTM3U\r\n#EXTINF:300,Old title\r\n..\\Music\\House\\a.mp3\r\n..\\Music\\House\\z.mp3\r\n";
    fs::write(&playlist, original).expect("playlist");
    let changes = [
        PathChange {
            old_path: text(&root.join("Music/House/a.mp3")),
            new_path: text(&root.join("Music/House/a.flac")),
            title: Some("Ada Marsh - First Light".to_owned()),
            duration_s: Some(301),
        },
        PathChange {
            old_path: text(&root.join("Music/House/gone.mp3")),
            new_path: text(&root.join("Music/House/gone.flac")),
            title: None,
            duration_s: None,
        },
    ];

    let plan = m3u::M3u.preview_changes(&playlist, &changes).expect("plan");
    assert_eq!(plan.edit_count(), 2);
    assert_eq!(plan.files[0].edits[1].after, r"..\Music\House\a.flac");
    assert_eq!(plan.not_found, [changes[1].old_path.clone()]);
    assert_eq!(fs::read_to_string(&playlist).expect("read"), original);

    let backups = root.join("backups");
    let applied = m3u::M3u
        .apply_changes(&playlist, &changes, &backups)
        .expect("applied");
    assert_eq!(applied.edits, 2);
    assert_eq!(applied.backups.len(), 1);
    assert_eq!(
        fs::read_to_string(&applied.backups[0]).expect("backup"),
        original
    );
    assert_eq!(
        fs::read_to_string(&playlist).expect("rewritten"),
        "#EXTM3U\r\n#EXTINF:301,Ada Marsh - First Light\r\n..\\Music\\House\\a.flac\r\n..\\Music\\House\\z.mp3\r\n"
    );
    assert!(!root.join("Sets/Friday.m3u8.resonance-tmp").exists());

    let again = m3u::M3u
        .apply_changes(&playlist, &changes, &backups)
        .expect("nothing left to change");
    assert_eq!(again.edits, 0);
    assert!(again.backups.is_empty());
}

#[test]
fn a_latin1_playlist_that_cannot_hold_the_new_path_is_left_alone() {
    let root = sandbox("m3u-latin1");
    let playlist = root.join("Sets/Old.m3u");
    fs::write(&playlist, b"Caf\xE9.mp3\n").expect("playlist");
    let changes = [PathChange {
        old_path: text(&root.join("Sets/Café.mp3")),
        new_path: text(&root.join("Sets/Ωmega.flac")),
        title: None,
        duration_s: None,
    }];
    let refused = m3u::M3u.apply_changes(&playlist, &changes, &root.join("backups"));
    assert!(matches!(refused, Err(IntegrationError::Text { .. })));
    assert_eq!(fs::read(&playlist).expect("read"), b"Caf\xE9.mp3\n");
    assert!(!root.join("backups").exists());
}

#[test]
fn an_export_reads_back_and_never_overwrites() {
    let root = sandbox("m3u-export");
    let out = root.join("Sets/Warm up.m3u8");
    let tracks = [PlaylistTrack {
        path: text(&root.join("Music/House/a.flac")),
        title: "Ada Marsh - First Light".to_owned(),
        duration_s: Some(301),
    }];
    m3u::export(&out, &tracks).expect("exported");
    let library = m3u::M3u.read_library(&out).expect("read");
    assert_eq!(library.playlists[0].entries[0].path, tracks[0].path);
    assert!(matches!(
        m3u::export(&out, &tracks),
        Err(IntegrationError::AlreadyExists { .. })
    ));
    assert!(matches!(
        m3u::export(&root.join("Sets/x.txt"), &tracks),
        Err(IntegrationError::NotAPlaylist { .. })
    ));
}
