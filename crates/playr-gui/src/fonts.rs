//! System fonts for the scripts egui's own fonts lack, as CJK, Arabic or Thai.
//!
//! egui's fonts cover Latin, Greek and Cyrillic, and draw anything else as a
//! box. When the library's text changes, the characters egui cannot draw are
//! looked up in the system's fonts, and each font that covers some of them is
//! added as a fallback. A library with no such characters reads no font file.
//! The candidates are fixed files on macOS and Windows, and fontconfig's
//! choice on Linux. Bundling fonts would draw the same everywhere, at 10 MB
//! or more for CJK alone.

use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

use eframe::egui;
use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use playr_app::dispatch::Frontend;
use playr_app::model::Model;
use skrifa::MetadataProvider;

/// Characters passed to fontconfig at once, to bound its command line.
#[cfg(not(any(target_os = "macos", windows)))]
const QUERY: usize = 256;

/// A font file, and the face in it for a collection.
type Face = (PathBuf, u32);

/// What has been looked for, so a change to the library looks only for new characters.
#[derive(Default)]
pub struct Fallbacks {
    /// The text last looked at, by `key`.
    seen: Option<(u64, usize, usize, usize)>,
    /// Faces read already, added or not.
    tried: HashSet<Face>,
    /// Characters no font was found for.
    hopeless: HashSet<char>,
}

impl Fallbacks {
    /// Adds the fonts the model's text needs, when that text has changed.
    pub fn update(&mut self, ctx: &egui::Context, model: &Model) {
        let key = key(model);
        if self.seen == Some(key) {
            return;
        }
        self.seen = Some(key);
        let mut chars = HashSet::new();
        for text in texts(model) {
            chars.extend(text.chars().filter(|&c| !c.is_ascii() && !invisible(c)));
        }
        chars.retain(|c| !self.hopeless.contains(c));
        if chars.is_empty() {
            return;
        }
        let font = egui::FontId::proportional(14.0);
        let missing: BTreeSet<char> = ctx.fonts_mut(|f| {
            chars
                .into_iter()
                .filter(|&c| !f.has_glyph(&font, c))
                .collect()
        });
        let faces = candidates(&missing);
        for (name, data) in self.find(missing, faces) {
            ctx.add_font(FontInsert::new(
                &name,
                data,
                [egui::FontFamily::Proportional, egui::FontFamily::Monospace]
                    .into_iter()
                    .map(|family| InsertFontFamily {
                        family,
                        priority: FontPriority::Lowest,
                    })
                    .collect(),
            ));
        }
    }

    /// The faces in `faces` that cover any of `missing`, by name. Each face is
    /// read once; what none covers is not looked for again.
    fn find(
        &mut self,
        mut missing: BTreeSet<char>,
        faces: Vec<Face>,
    ) -> Vec<(String, egui::FontData)> {
        let mut found = Vec::new();
        for face in faces {
            if missing.is_empty() {
                break;
            }
            if !self.tried.insert(face.clone()) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&face.0) else {
                continue;
            };
            let covered = covered(&bytes, face.1, &missing);
            if covered.is_empty() {
                continue;
            }
            missing.retain(|c| !covered.contains(c));
            // egui copies owned font data into its own blob, so 55 MB of
            // Korean held 110; static data it borrows. Each face is read once
            // and kept for the window's life, so the leak is bounded.
            let mut data = egui::FontData::from_static(Box::leak(bytes.into_boxed_slice()));
            data.index = face.1;
            found.push((format!("{}#{}", face.0.display(), face.1), data));
        }
        self.hopeless.extend(missing);
        found
    }
}

/// Characters drawn as nothing, which no font needs loading for: controls,
/// spaces, joiners, direction marks, the soft hyphen and variation selectors.
fn invisible(c: char) -> bool {
    c.is_control()
        || c.is_whitespace()
        || matches!(c, '\u{ad}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}' | '\u{fe00}'..='\u{fe0f}' | '\u{feff}')
}

/// The characters of `chars` the face covers; none when it cannot be read.
fn covered(bytes: &[u8], index: u32, chars: &BTreeSet<char>) -> Vec<char> {
    let Ok(font) = skrifa::FontRef::from_index(bytes, index) else {
        return Vec::new();
    };
    let map = font.charmap();
    chars
        .iter()
        .copied()
        .filter(|&c| map.map(c).is_some())
        .collect()
}

/// What changes when the text `texts` yields may have.
fn key(model: &Model) -> (u64, usize, usize, usize) {
    let session = model.session();
    (
        session.revision(),
        model.queue().len(),
        session.selection().len(),
        session.playlists().len() + session.searches().len(),
    )
}

/// The text the window shows from the library: tags, paths and list names.
fn texts(model: &Model) -> impl Iterator<Item = &str> {
    let session = model.session();
    let tracks = session
        .tracks()
        .iter()
        .chain(model.queue())
        .chain(session.selection());
    let tags = tracks.flat_map(|t| {
        [&t.title, &t.artist, &t.album, &t.album_artist, &t.genre]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .chain([t.path.as_str()])
    });
    let names = (session.playlists().iter().map(|p| p.name.as_str()))
        .chain(session.searches().iter().map(|s| s.name.as_str()));
    tags.chain(names)
}

/// A script's characters, and the font files that may cover them, best first.
#[cfg(any(target_os = "macos", windows))]
type Script = (
    &'static [std::ops::RangeInclusive<char>],
    &'static [&'static str],
);

#[cfg(any(target_os = "macos", windows))]
const CJK: &[std::ops::RangeInclusive<char>] = &[
    '\u{3000}'..='\u{30ff}',
    '\u{3400}'..='\u{4dbf}',
    '\u{4e00}'..='\u{9fff}',
    '\u{f900}'..='\u{faff}',
    '\u{ff00}'..='\u{ffef}',
    '\u{20000}'..='\u{3134f}',
];
#[cfg(any(target_os = "macos", windows))]
const HANGUL: &[std::ops::RangeInclusive<char>] = &[
    '\u{1100}'..='\u{11ff}',
    '\u{3130}'..='\u{318f}',
    '\u{ac00}'..='\u{d7ff}',
];
#[cfg(any(target_os = "macos", windows))]
#[allow(clippy::single_range_in_vec_init)] // One range of several a script may have.
const THAI: &[std::ops::RangeInclusive<char>] = &['\u{0e00}'..='\u{0e7f}'];
#[cfg(any(target_os = "macos", windows))]
const ARABIC: &[std::ops::RangeInclusive<char>] = &[
    '\u{0600}'..='\u{06ff}',
    '\u{0750}'..='\u{077f}',
    '\u{fb50}'..='\u{fdff}',
    '\u{fe70}'..='\u{feff}',
];
#[cfg(any(target_os = "macos", windows))]
#[allow(clippy::single_range_in_vec_init)] // One range of several a script may have.
const HEBREW: &[std::ops::RangeInclusive<char>] = &['\u{0590}'..='\u{05ff}'];
/// Devanagari to Malayalam.
#[cfg(any(target_os = "macos", windows))]
#[allow(clippy::single_range_in_vec_init)] // One range of several a script may have.
const INDIC: &[std::ops::RangeInclusive<char>] = &['\u{0900}'..='\u{0dff}'];

/// The files of each script `missing` has a character of, every script's
/// first choice before any second, so a large fallback of one script is not
/// loaded for another's characters; then `other` for characters in none.
#[cfg(any(target_os = "macos", windows))]
fn pick(scripts: &[Script], other: &[&'static str], missing: &BTreeSet<char>) -> Vec<&'static str> {
    let of = |c: char| {
        scripts
            .iter()
            .find(|(ranges, _)| ranges.iter().any(|r| r.contains(&c)))
    };
    let present: Vec<_> = (scripts.iter())
        .filter(|(ranges, _)| missing.iter().any(|c| ranges.iter().any(|r| r.contains(c))))
        .map(|(_, list)| *list)
        .collect();
    let longest = present.iter().map(|list| list.len()).max().unwrap_or(0);
    let mut files: Vec<&str> = (0..longest)
        .flat_map(|rank| {
            present
                .iter()
                .filter_map(move |list| list.get(rank).copied())
        })
        .collect();
    if missing.iter().any(|&c| of(c).is_none()) {
        files.extend_from_slice(other);
    }
    let mut seen = HashSet::new();
    files.retain(|f| seen.insert(*f));
    files
}

/// Fonts that may cover `missing`, best first: only those of its scripts, as
/// a CJK file is 8 to 56 MB. LastResort.otf is left out: it maps every
/// character to a placeholder.
#[cfg(target_os = "macos")]
fn candidates(missing: &BTreeSet<char>) -> Vec<Face> {
    const DIR: &str = "/System/Library/Fonts/";
    const SCRIPTS: &[Script] = &[
        // Japanese first, the smallest at 7.9 MB; then Chinese.
        (
            CJK,
            &[
                "ヒラギノ角ゴシック W3.ttc",
                "Hiragino Sans GB.ttc",
                "STHeiti Light.ttc",
            ],
        ),
        // 15 MB, then the 55 MB collection for anything it lacks.
        (
            HANGUL,
            &["Supplemental/AppleGothic.ttf", "AppleSDGothicNeo.ttc"],
        ),
        (THAI, &["ThonburiUI.ttc"]),
        (ARABIC, &["SFArabic.ttf", "GeezaPro.ttc"]),
        (HEBREW, &["SFHebrew.ttf"]),
        (
            INDIC,
            &[
                "Kohinoor.ttc",
                "KohinoorBangla.ttc",
                "MuktaMahee.ttc",
                "KohinoorGujarati.ttc",
                "NotoSansOriya.ttc",
                "KohinoorTelugu.ttc",
                "NotoSansKannada.ttc",
            ],
        ),
    ];
    // Geneva first, at 719 KB, for the marks and punctuation of Latin text
    // that egui's font lacks; Arial Unicode last, wide, at 22 MB.
    const OTHER: &[&str] = &[
        "Geneva.ttf",
        "SFArmenian.ttf",
        "SFGeorgian.ttf",
        "NotoSansMyanmar.ttc",
        "Apple Symbols.ttf",
        "Supplemental/Arial Unicode.ttf",
    ];
    (pick(SCRIPTS, OTHER, missing).into_iter())
        .map(|f| (PathBuf::from(DIR).join(f), 0))
        .collect()
}

/// Fonts that may cover `missing`, best first: only those of its scripts.
#[cfg(windows)]
fn candidates(missing: &BTreeSet<char>) -> Vec<Face> {
    const SCRIPTS: &[Script] = &[
        (CJK, &["YuGothR.ttc", "msyh.ttc", "msjh.ttc"]),
        (HANGUL, &["malgun.ttf"]),
        (THAI, &["LeelawUI.ttf"]),
        (ARABIC, &["segoeui.ttf"]),
        (HEBREW, &["segoeui.ttf"]),
        (INDIC, &["Nirmala.ttc", "Nirmala.ttf"]),
    ];
    const OTHER: &[&str] = &["segoeui.ttf", "seguisym.ttf", "ebrima.ttf"];
    let dir =
        std::env::var_os("WINDIR").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    (pick(SCRIPTS, OTHER, missing).into_iter())
        .map(|f| (dir.join("Fonts").join(f), 0))
        .collect()
}

/// fontconfig's fonts for `missing`, each adding to what those before it
/// cover. LastResort is left out: it maps every character to a placeholder.
#[cfg(not(any(target_os = "macos", windows)))]
fn candidates(missing: &BTreeSet<char>) -> Vec<Face> {
    let charset: Vec<String> = (missing.iter().take(QUERY))
        .map(|&c| format!("{:x}", c as u32))
        .collect();
    let output = std::process::Command::new("fc-match")
        .args(["-s", "--format", "%{file}\t%{index}\n"])
        .arg(format!(":charset={}", charset.join(" ")))
        .output();
    let Some(output) = output.ok().filter(|o| o.status.success()) else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (file, index) = line.split_once('\t')?;
            let index = index.parse().ok()?;
            (!file.contains("LastResort")).then(|| (PathBuf::from(file), index))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// egui's own Latin font, as a face on disk.
    fn latin(dir: &std::path::Path) -> Face {
        let fonts = egui::FontDefinitions::default();
        let path = dir.join("latin.ttf");
        std::fs::write(&path, &*fonts.font_data["Ubuntu-Light"].font).unwrap();
        (path, 0)
    }

    #[test]
    fn a_face_is_added_for_what_it_covers_and_a_character_none_covers_is_given_up() {
        let dir = tempfile::tempdir().unwrap();
        let face = latin(dir.path());
        let mut fallbacks = Fallbacks::default();
        let found = fallbacks.find(BTreeSet::from(['a', 'b', '\u{4e2d}']), vec![face.clone()]);
        assert_eq!(found.len(), 1, "one face, for a and b");
        assert_eq!(found[0].1.index, 0);
        assert_eq!(fallbacks.hopeless, HashSet::from(['\u{4e2d}']));

        // A face read once is not read again.
        let again = fallbacks.find(BTreeSet::from(['c']), vec![face]);
        assert!(again.is_empty());
    }

    #[test]
    fn a_missing_or_unreadable_file_is_passed_over() {
        let dir = tempfile::tempdir().unwrap();
        let junk = dir.path().join("junk.ttf");
        std::fs::write(&junk, b"not a font").unwrap();
        let faces = vec![
            (dir.path().join("absent.ttf"), 0),
            (junk, 0),
            latin(dir.path()),
        ];
        let found = Fallbacks::default().find(BTreeSet::from(['a']), faces);
        assert_eq!(found.len(), 1);
    }

    #[cfg(any(target_os = "macos", windows))]
    #[test]
    fn only_the_scripts_present_are_read() {
        const SCRIPTS: &[Script] = &[(CJK, &["cjk", "cjk2"]), (THAI, &["thai", "cjk"])];
        let thai = BTreeSet::from(['\u{e44}']);
        assert_eq!(pick(SCRIPTS, &["other"], &thai), ["thai", "cjk"]);
        let mixed = BTreeSet::from(['\u{4e2d}', '\u{e44}', '\u{308}']);
        assert_eq!(
            pick(SCRIPTS, &["other"], &mixed),
            ["cjk", "thai", "cjk2", "other"]
        );
    }
}
