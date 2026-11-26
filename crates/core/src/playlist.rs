//! M3U and M3U8 playlists as text, and the path rules every external library shares: how a
//! path another app wrote is read, compared and written back in that app's own style.

use std::fmt;

/// A path with forward slashes, no `.` or `..` segments and no trailing slash. Drive letters
/// are upper case. Nothing is trimmed, because folder names can end in a space.
pub fn normalise_path(raw: &str) -> String {
    let unified = raw.replace('\\', "/");
    let (root, rest) = split_root(&unified);
    let mut segments: Vec<&str> = Vec::new();
    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.last().is_some_and(|last| *last != "..") {
                    segments.pop();
                } else if root.is_empty() {
                    segments.push("..");
                }
            }
            other => segments.push(other),
        }
    }
    let joined = segments.join("/");
    if root.is_empty() {
        joined
    } else if joined.is_empty() {
        root.trim_end_matches('/').to_owned() + "/"
    } else {
        format!("{root}{joined}")
    }
}

/// The root of a path with forward slashes, and the rest. `C:/`, `//server/share/`, `/` or none.
fn split_root(path: &str) -> (String, &str) {
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let drive = (bytes[0] as char).to_ascii_uppercase();
        let rest = path[2..].trim_start_matches('/');
        return (format!("{drive}:/"), rest);
    }
    if let Some(unc) = path.strip_prefix("//") {
        let mut parts = unc.splitn(3, '/');
        let server = parts.next().unwrap_or_default();
        let share = parts.next().unwrap_or_default();
        let rest = parts.next().unwrap_or_default();
        return (format!("//{server}/{share}/"), rest);
    }
    if let Some(rest) = path.strip_prefix('/') {
        return ("/".to_owned(), rest);
    }
    (String::new(), path)
}

/// Whether `raw` names a file from a root rather than from the playlist's folder.
pub fn is_absolute_path(raw: &str) -> bool {
    !split_root(&raw.replace('\\', "/")).0.is_empty()
}

/// What two paths are compared by. Windows and macOS file systems ignore case by default.
pub fn path_key(path: &str) -> String {
    let normal = normalise_path(path);
    if cfg!(any(windows, target_os = "macos")) {
        normal.to_lowercase()
    } else {
        normal
    }
}

/// Whether `a` and `b` name the same file, by [`path_key`].
pub fn same_path(a: &str, b: &str) -> bool {
    path_key(a) == path_key(b)
}

/// `path` under `folder`, as the part after it with forward slashes, if it is inside it.
pub fn path_inside(folder: &str, path: &str) -> Option<String> {
    let folder = normalise_path(folder);
    let path = normalise_path(path);
    let folder_key = path_key(&folder);
    let path_key = path_key(&path);
    let prefix = if folder_key.ends_with('/') {
        folder_key
    } else {
        format!("{folder_key}/")
    };
    // Keys keep the length of what they were made from, except where lowering changes the
    // length of a character, so the remainder is cut from the normalised path by characters.
    if !path_key.starts_with(&prefix) {
        return None;
    }
    let skip = prefix.chars().count();
    let rest: String = path.chars().skip(skip).collect();
    (!rest.is_empty()).then_some(rest)
}

/// `target` written from `folder` with `..` where needed, or `None` across roots.
pub fn relative_path(folder: &str, target: &str) -> Option<String> {
    let folder = normalise_path(folder);
    let target = normalise_path(target);
    let (folder_root, folder_rest) = split_root(&folder);
    let (target_root, target_rest) = split_root(&target);
    if folder_root.is_empty() || path_key(&folder_root) != path_key(&target_root) {
        return None;
    }
    let from: Vec<&str> = folder_rest.split('/').filter(|s| !s.is_empty()).collect();
    let to: Vec<&str> = target_rest.split('/').filter(|s| !s.is_empty()).collect();
    let shared = from
        .iter()
        .zip(&to)
        .take_while(|(a, b)| path_key(a) == path_key(b))
        .count();
    let mut parts: Vec<&str> = vec![".."; from.len() - shared];
    parts.extend(&to[shared..]);
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// A playlist line's path as a full path, or `None` for a stream or other non-file address.
pub fn resolve_entry(playlist_folder: &str, raw: &str) -> Option<String> {
    let raw = raw.trim_start();
    if raw.is_empty() {
        return None;
    }
    if let Some(url) = strip_prefix_ignore_case(raw, "file://") {
        // `file:///C:/x` and `file:///home/x` both start the path after the third slash; the
        // POSIX form keeps it.
        let path = url.strip_prefix("localhost").unwrap_or(url);
        let decoded = percent_decode(path)?;
        let bytes = decoded.as_bytes();
        let path = if bytes.len() >= 3
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':'
        {
            &decoded[1..]
        } else {
            &decoded
        };
        return Some(normalise_path(path));
    }
    if has_url_scheme(raw) {
        return None;
    }
    if is_absolute_path(raw) {
        Some(normalise_path(raw))
    } else {
        Some(normalise_path(&format!("{playlist_folder}/{raw}")))
    }
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// `http://`, `rtsp://` and the like. A drive letter is one character, so `C:` is not a scheme.
fn has_url_scheme(raw: &str) -> bool {
    raw.split_once("://").is_some_and(|(scheme, _)| {
        scheme.len() > 1
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    })
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// How a playlist file's text was stored, kept so a rewrite stores it the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8 {
        bom: bool,
    },
    /// Older `.m3u` files written in a Western code page.
    Latin1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaylistTextError {
    /// A Latin-1 playlist cannot hold a character of a new path or title.
    NotLatin1 { line: usize },
}

impl fmt::Display for PlaylistTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotLatin1 { line } => write!(
                f,
                "Line {} needs characters this older playlist cannot store. Export the \
                 collection to M3U8 instead.",
                line + 1
            ),
        }
    }
}

impl std::error::Error for PlaylistTextError {}

/// The `#EXTINF` line before a path: the length in seconds and the title shown.
#[derive(Debug, Clone, PartialEq)]
pub struct Extinf {
    pub line: usize,
    pub duration_s: Option<f64>,
    pub title: Option<String>,
}

/// A line that names a file or stream, with the `#EXTINF` that describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct M3uEntry {
    pub line: usize,
    pub raw: String,
    pub extinf: Option<Extinf>,
}

/// A playlist file as lines, so a rewrite changes only the lines it means to and keeps the
/// encoding and line endings it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M3uDocument {
    lines: Vec<String>,
    encoding: TextEncoding,
    crlf: bool,
    final_newline: bool,
}

impl M3uDocument {
    pub fn decode(bytes: &[u8]) -> Self {
        let (text, encoding) = match bytes.strip_prefix(b"\xEF\xBB\xBF") {
            Some(rest) => (
                String::from_utf8_lossy(rest).into_owned(),
                TextEncoding::Utf8 { bom: true },
            ),
            None => match std::str::from_utf8(bytes) {
                Ok(text) => (text.to_owned(), TextEncoding::Utf8 { bom: false }),
                Err(_) => (
                    bytes.iter().map(|&byte| byte as char).collect(),
                    TextEncoding::Latin1,
                ),
            },
        };
        let crlf = text.contains("\r\n");
        let final_newline = text.ends_with('\n');
        let body = text.strip_suffix('\n').unwrap_or(&text);
        let lines = if body.is_empty() && !final_newline {
            Vec::new()
        } else {
            body.split('\n')
                .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
                .collect()
        };
        Self {
            lines,
            encoding,
            crlf,
            final_newline,
        }
    }

    pub fn encoding(&self) -> TextEncoding {
        self.encoding
    }

    pub fn line(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(String::as_str)
    }

    pub fn set_line(&mut self, index: usize, text: String) {
        if let Some(line) = self.lines.get_mut(index) {
            *line = text;
        }
    }

    pub fn entries(&self) -> Vec<M3uEntry> {
        let mut entries = Vec::new();
        let mut extinf: Option<Extinf> = None;
        for (index, line) in self.lines.iter().enumerate() {
            let text = line.trim_start();
            if text.is_empty() {
                continue;
            }
            if let Some(info) = strip_prefix_ignore_case(text, "#EXTINF:") {
                let (duration_s, _, title) = split_extinf(info);
                extinf = Some(Extinf {
                    line: index,
                    duration_s,
                    title,
                });
            } else if !text.starts_with('#') {
                entries.push(M3uEntry {
                    line: index,
                    raw: line.clone(),
                    extinf: extinf.take(),
                });
            }
        }
        entries
    }

    pub fn encode(&self) -> Result<Vec<u8>, PlaylistTextError> {
        let newline = if self.crlf { "\r\n" } else { "\n" };
        let mut text = self.lines.join(newline);
        if self.final_newline {
            text.push_str(newline);
        }
        match self.encoding {
            TextEncoding::Utf8 { bom } => {
                let mut bytes = Vec::with_capacity(text.len() + 3);
                if bom {
                    bytes.extend_from_slice(b"\xEF\xBB\xBF");
                }
                bytes.extend_from_slice(text.as_bytes());
                Ok(bytes)
            }
            TextEncoding::Latin1 => {
                let mut bytes = Vec::with_capacity(text.len());
                let mut line = 0;
                for c in text.chars() {
                    if c == '\n' {
                        line += 1;
                    }
                    bytes.push(u8::try_from(u32::from(c)).map_err(|_| {
                        PlaylistTextError::NotLatin1 {
                            line: line.min(self.lines.len().saturating_sub(1)),
                        }
                    })?);
                }
                Ok(bytes)
            }
        }
    }
}

/// `123 tvg-id="x",Artist - Title` into the length, what sits between it and the comma, and the
/// title. A comma inside quotes belongs to an attribute.
fn split_extinf(info: &str) -> (Option<f64>, &str, Option<String>) {
    let mut quoted = false;
    let mut comma = None;
    for (index, c) in info.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                comma = Some(index);
                break;
            }
            _ => {}
        }
    }
    let (head, title) = match comma {
        Some(index) => (&info[..index], Some(info[index + 1..].to_owned())),
        None => (info, None),
    };
    let head = head.trim_start();
    let length_end = head.find(|c: char| c.is_whitespace()).unwrap_or(head.len());
    let duration_s = head[..length_end]
        .parse::<f64>()
        .ok()
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0);
    let title = title.filter(|title| !title.trim().is_empty());
    (duration_s, &head[length_end..], title)
}

/// A file a library should name by a new path, with what it should show for it.
#[derive(Debug, Clone, PartialEq)]
pub struct PathChange {
    pub old_path: String,
    pub new_path: String,
    pub title: Option<String>,
    pub duration_s: Option<u32>,
}

/// One line of a playlist before and after a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEdit {
    pub line: usize,
    pub before: String,
    pub after: String,
}

/// The new path in the style the playlist used for the old one: relative where it was
/// relative and still can be, and with backslashes where it had them.
fn styled_path(playlist_folder: &str, original: &str, new_path: &str) -> String {
    let written = if is_absolute_path(original)
        || strip_prefix_ignore_case(original.trim_start(), "file://").is_some()
    {
        normalise_path(new_path)
    } else {
        relative_path(playlist_folder, new_path).unwrap_or_else(|| normalise_path(new_path))
    };
    if original.contains('\\') {
        written.replace('/', "\\")
    } else {
        written
    }
}

fn extinf_line(original: &str, change: &PathChange) -> String {
    let info = strip_prefix_ignore_case(original.trim_start(), "#EXTINF:").unwrap_or_default();
    let (duration_s, attributes, title) = split_extinf(info);
    let length = match (change.duration_s, duration_s) {
        (Some(seconds), _) => seconds.to_string(),
        (None, Some(seconds)) => format!("{}", seconds.round()),
        (None, None) => "-1".to_owned(),
    };
    let title = change.title.clone().or(title).unwrap_or_default();
    format!("#EXTINF:{length}{attributes},{title}")
}

/// The edits that make a playlist name each changed file by its new path. A file the playlist
/// names more than once is changed everywhere.
pub fn plan_path_changes(
    document: &M3uDocument,
    playlist_folder: &str,
    changes: &[PathChange],
) -> Vec<LineEdit> {
    let mut edits = Vec::new();
    for entry in document.entries() {
        let Some(resolved) = resolve_entry(playlist_folder, &entry.raw) else {
            continue;
        };
        let Some(change) = changes
            .iter()
            .find(|change| same_path(&change.old_path, &resolved))
        else {
            continue;
        };
        if let Some(extinf) = &entry.extinf
            && let Some(before) = document.line(extinf.line)
        {
            let after = extinf_line(before, change);
            if after != before {
                edits.push(LineEdit {
                    line: extinf.line,
                    before: before.to_owned(),
                    after,
                });
            }
        }
        let after = styled_path(playlist_folder, &entry.raw, &change.new_path);
        if after != entry.raw {
            edits.push(LineEdit {
                line: entry.line,
                before: entry.raw.clone(),
                after,
            });
        }
    }
    edits
}

/// A file a new playlist names.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaylistTrack {
    pub path: String,
    pub title: String,
    pub duration_s: Option<u32>,
}

/// An extended M3U8 playlist naming each file by its full path, in the platform's own slashes.
pub fn render_m3u8(tracks: &[PlaylistTrack]) -> String {
    let mut text = String::from("#EXTM3U\n");
    for track in tracks {
        let length = track
            .duration_s
            .map_or_else(|| "-1".to_owned(), |seconds| seconds.to_string());
        let title: String = track.title.chars().filter(|c| !c.is_control()).collect();
        let path = normalise_path(&track.path);
        let path = if cfg!(windows) {
            path.replace('/', "\\")
        } else {
            path
        };
        text.push_str(&format!("#EXTINF:{length},{title}\n{path}\n"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_normalise_across_slash_styles_and_dot_segments() {
        assert_eq!(
            normalise_path(r"c:\Music\House\.\a.mp3"),
            "C:/Music/House/a.mp3"
        );
        assert_eq!(
            normalise_path("/home/dj/Music/../Crates//b.flac"),
            "/home/dj/Crates/b.flac"
        );
        assert_eq!(
            normalise_path(r"\\nas\share\Music\c.wav"),
            "//nas/share/Music/c.wav"
        );
        assert_eq!(normalise_path("../up/d.mp3"), "../up/d.mp3");
        assert_eq!(normalise_path("C:/"), "C:/");
        assert_eq!(normalise_path("Solid Kick /e.mp3"), "Solid Kick /e.mp3");
    }

    #[test]
    fn entries_resolve_against_the_playlist_folder() {
        let folder = "D:/Sets";
        assert_eq!(
            resolve_entry(folder, r"..\Music\a.mp3").as_deref(),
            Some("D:/Music/a.mp3")
        );
        assert_eq!(
            resolve_entry(folder, "Tracks/b.flac").as_deref(),
            Some("D:/Sets/Tracks/b.flac")
        );
        assert_eq!(
            resolve_entry(folder, "/srv/music/c.wav").as_deref(),
            Some("/srv/music/c.wav")
        );
        assert_eq!(
            resolve_entry(folder, "file:///E:/My%20Music/d.mp3").as_deref(),
            Some("E:/My Music/d.mp3")
        );
        assert_eq!(
            resolve_entry(folder, "file:///home/dj/e.mp3").as_deref(),
            Some("/home/dj/e.mp3")
        );
        assert_eq!(resolve_entry(folder, "http://radio.example/stream"), None);
    }

    #[test]
    fn relative_paths_climb_out_of_the_playlist_folder_and_stop_at_roots() {
        assert_eq!(
            relative_path("D:/Sets/Friday", "D:/Music/a.flac").as_deref(),
            Some("../../Music/a.flac")
        );
        assert_eq!(
            relative_path("D:/Sets", "D:/Sets/a.flac").as_deref(),
            Some("a.flac")
        );
        assert_eq!(relative_path("D:/Sets", "E:/Music/a.flac"), None);
        assert_eq!(
            path_inside("D:/Music", "D:/Music/House/a.mp3").as_deref(),
            Some("House/a.mp3")
        );
        assert_eq!(path_inside("D:/Music", "D:/Musical/a.mp3"), None);
    }

    #[test]
    fn extended_entries_carry_their_length_and_title() {
        let document = M3uDocument::decode(
            b"#EXTM3U\r\n#EXTINF:312 tvg-logo=\"a,b\",Ada Marsh - First Light\r\nHouse\\a.mp3\r\n\r\n# note\r\nb.flac\r\n",
        );
        let entries = document.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].raw, r"House\a.mp3");
        let extinf = entries[0].extinf.as_ref().expect("EXTINF");
        assert_eq!(extinf.duration_s, Some(312.0));
        assert_eq!(extinf.title.as_deref(), Some("Ada Marsh - First Light"));
        assert_eq!(entries[1].extinf, None);
        assert_eq!(
            document.encode().expect("encodes"),
            b"#EXTM3U\r\n#EXTINF:312 tvg-logo=\"a,b\",Ada Marsh - First Light\r\nHouse\\a.mp3\r\n\r\n# note\r\nb.flac\r\n"
        );
    }

    #[test]
    fn a_path_change_keeps_the_playlist_style_and_refreshes_its_extinf() {
        let mut document = M3uDocument::decode(
            "#EXTM3U\n#EXTINF:200 group=\"x\",Old\n..\\Music\\a.mp3\nD:/Music/a.mp3\nD:/Music/z.mp3\n"
                .as_bytes(),
        );
        let changes = [PathChange {
            old_path: "D:/Music/a.mp3".to_owned(),
            new_path: "D:/Music/a.flac".to_owned(),
            title: Some("Artist - New".to_owned()),
            duration_s: Some(201),
        }];
        let edits = plan_path_changes(&document, "D:/Sets", &changes);
        assert_eq!(
            edits,
            [
                LineEdit {
                    line: 1,
                    before: "#EXTINF:200 group=\"x\",Old".to_owned(),
                    after: "#EXTINF:201 group=\"x\",Artist - New".to_owned(),
                },
                LineEdit {
                    line: 2,
                    before: r"..\Music\a.mp3".to_owned(),
                    after: r"..\Music\a.flac".to_owned(),
                },
                LineEdit {
                    line: 3,
                    before: "D:/Music/a.mp3".to_owned(),
                    after: "D:/Music/a.flac".to_owned(),
                },
            ]
        );
        for edit in edits {
            document.set_line(edit.line, edit.after);
        }
        assert_eq!(
            String::from_utf8(document.encode().expect("encodes")).expect("UTF-8"),
            "#EXTM3U\n#EXTINF:201 group=\"x\",Artist - New\n..\\Music\\a.flac\nD:/Music/a.flac\nD:/Music/z.mp3\n"
        );
    }

    #[test]
    fn a_latin1_playlist_stays_latin1_and_refuses_what_it_cannot_store() {
        let mut document = M3uDocument::decode(b"Ni\xF1o/a.mp3\n");
        assert_eq!(document.encoding(), TextEncoding::Latin1);
        assert_eq!(document.entries()[0].raw, "Niño/a.mp3");
        assert_eq!(document.encode().expect("encodes"), b"Ni\xF1o/a.mp3\n");
        document.set_line(0, "Ωmega/a.mp3".to_owned());
        assert_eq!(
            document.encode(),
            Err(PlaylistTextError::NotLatin1 { line: 0 })
        );
    }

    #[test]
    fn an_exported_playlist_is_extended_m3u8() {
        let text = render_m3u8(&[
            PlaylistTrack {
                path: "/srv/music/a.flac".to_owned(),
                title: "Ada Marsh - First Light".to_owned(),
                duration_s: Some(312),
            },
            PlaylistTrack {
                path: "/srv/music/b.mp3".to_owned(),
                title: "b".to_owned(),
                duration_s: None,
            },
        ]);
        let sep = if cfg!(windows) { "\\" } else { "/" };
        assert_eq!(
            text,
            format!(
                "#EXTM3U\n#EXTINF:312,Ada Marsh - First Light\n{sep}srv{sep}music{sep}a.flac\n#EXTINF:-1,b\n{sep}srv{sep}music{sep}b.mp3\n"
            )
        );
        let entries = M3uDocument::decode(text.as_bytes()).entries();
        assert_eq!(
            resolve_entry("/", &entries[0].raw).as_deref(),
            Some("/srv/music/a.flac")
        );
    }
}
