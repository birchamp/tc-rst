//! The one check for a relative file path that comes from outside the app (a zip entry, a
//! git tree, a resource's file list) before it is joined to a folder on disk. A name that
//! fails is refused, never repaired: repairing (`\` to `/`, trimming a trailing space) can
//! merge two entries into one file.

/// The first rule a path breaks. [`PathProblem::leaves_folder`] tells the rules that could
/// put a file outside the folder from those that only make the name unwritable, or another
/// file's name, on Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
pub enum PathProblem {
    #[error("the path is empty")]
    Empty,
    #[error("the path starts with '/'")]
    Absolute,
    #[error("the path contains '\\', a separator on Windows")]
    Backslash,
    /// `C:x` is relative to drive C's current folder.
    #[error("the path starts with a drive letter")]
    DrivePrefix,
    #[error("the path contains a NUL")]
    Nul,
    #[error("the path has an empty segment")]
    EmptySegment,
    #[error("the path has a '.' segment")]
    CurrentDir,
    /// `..`, and `...` or `.. ` too: Windows strips trailing dots and spaces.
    #[error("the path has a '..' segment, which leaves the folder")]
    ParentDir,
    #[error("the path contains a control character")]
    Control(char),
    /// `:` would write an NTFS alternate stream.
    #[error("the path contains {0:?}, which Windows does not allow in a name")]
    ForbiddenChar(char),
    #[error("a segment ends with '.', which Windows drops")]
    TrailingDot,
    #[error("a segment ends with a space, which Windows drops")]
    TrailingSpace,
    #[error("a segment is a device name Windows reserves")]
    ReservedName,
    /// Windows can open another file through its 8.3 short name, such as `PROGRA~1`.
    #[error("a segment has the shape of a Windows short name")]
    ShortName,
}

impl PathProblem {
    /// Whether a path with this problem could name a file outside the folder it is joined
    /// to, on some platform.
    pub fn leaves_folder(self) -> bool {
        matches!(
            self,
            Self::Empty
                | Self::Absolute
                | Self::Backslash
                | Self::DrivePrefix
                | Self::Nul
                | Self::EmptySegment
                | Self::CurrentDir
                | Self::ParentDir
        )
    }
}

/// Checks a relative path that uses `/` between segments. Refused: anything that could
/// leave the folder (an empty path or segment, a leading `/`, `\`, a drive prefix, NUL, a
/// `.` or `..` segment), then anything Windows cannot write as named or would write as
/// another file: a control character, one of `: ? * " < > |`, a segment ending in a dot
/// or a space, a device name such as `CON` or `con .txt`, or an 8.3 short name. A problem
/// that leaves the folder is always the one reported when there is one.
pub fn check_path(path: &str) -> Result<(), PathProblem> {
    leaves_folder(path).map_or_else(|| windows_problem(path).map_or(Ok(()), Err), Err)
}

fn leaves_folder(path: &str) -> Option<PathProblem> {
    let bytes = path.as_bytes();
    if path.is_empty() {
        Some(PathProblem::Empty)
    } else if path.starts_with('/') {
        Some(PathProblem::Absolute)
    } else if path.contains('\\') {
        Some(PathProblem::Backslash)
    } else if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        Some(PathProblem::DrivePrefix)
    } else if path.contains('\0') {
        Some(PathProblem::Nul)
    } else {
        path.split('/').find_map(|segment| {
            if segment.is_empty() {
                Some(PathProblem::EmptySegment)
            } else if segment == "." {
                Some(PathProblem::CurrentDir)
            } else if segment.starts_with("..") && segment.bytes().all(|b| matches!(b, b'.' | b' '))
            {
                Some(PathProblem::ParentDir)
            } else {
                None
            }
        })
    }
}

fn windows_problem(path: &str) -> Option<PathProblem> {
    path.split('/').find_map(|segment| {
        if let Some(c) = segment.chars().find(|c| c.is_control()) {
            Some(PathProblem::Control(c))
        } else if let Some(c) = segment
            .chars()
            .find(|c| matches!(c, ':' | '?' | '*' | '"' | '<' | '>' | '|'))
        {
            Some(PathProblem::ForbiddenChar(c))
        } else if segment.ends_with('.') {
            Some(PathProblem::TrailingDot)
        } else if segment.ends_with(' ') {
            Some(PathProblem::TrailingSpace)
        } else if is_short_name(segment) {
            Some(PathProblem::ShortName)
        } else if is_windows_device(segment.split('.').next().unwrap_or_default().trim_end()) {
            Some(PathProblem::ReservedName)
        } else {
            None
        }
    })
}

/// Whether `segment` is shaped like the 8.3 short name Windows gives a long name, as in
/// `PROGRA~1` or `ABCDEF~1.JSN`: at most one dot; before it at most eight characters, made
/// of one to six characters other than `~`, then `~` and digits; after it at most three
/// characters. Only such a name can be one Windows made, so `notes~1.json` (a four-letter
/// extension) is a long name of its own.
fn is_short_name(segment: &str) -> bool {
    let (stem, extension) = segment.split_once('.').unwrap_or((segment, ""));
    let Some((base, number)) = stem.split_once('~') else {
        return false;
    };
    !extension.contains('.')
        && extension.chars().count() <= 3
        && stem.chars().count() <= 8
        && (1..=6).contains(&base.chars().count())
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
}

/// `CON`, `PRN`, `AUX`, `NUL`, `COM0`-`COM9`, `LPT0`-`LPT9` (also with superscript ¹²³,
/// which Windows reads as digits), `CONIN$` and `CONOUT$`, in any case.
fn is_windows_device(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let Some(rest) = upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
    else {
        return false;
    };
    let mut chars = rest.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some('0'..='9' | '¹' | '²' | '³'), None)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use PathProblem::*;

    #[test]
    fn every_rule_refuses_its_names() {
        for (path, problem) in [
            ("", Empty),
            ("/abs", Absolute),
            ("/", Absolute),
            ("a\\b", Backslash),
            ("c:\\x", Backslash),
            ("c:x", DrivePrefix),
            ("C:/x", DrivePrefix),
            ("a\0b", Nul),
            ("a//b", EmptySegment),
            ("a/", EmptySegment),
            ("./a", CurrentDir),
            ("a/./b", CurrentDir),
            ("..", ParentDir),
            ("../evil", ParentDir),
            ("a/.. /b", ParentDir),
            ("a/.../b", ParentDir),
            ("a/..  ", ParentDir),
            ("a\nb", Control('\n')),
            ("a\u{85}b", Control('\u{85}')),
            ("xy:z", ForbiddenChar(':')),
            ("x/c:d", ForbiddenChar(':')),
            ("a?b", ForbiddenChar('?')),
            ("a*b", ForbiddenChar('*')),
            ("a\"b", ForbiddenChar('"')),
            ("a<b", ForbiddenChar('<')),
            ("a>b", ForbiddenChar('>')),
            ("a|b", ForbiddenChar('|')),
            ("book.usfm.", TrailingDot),
            ("dir./x", TrailingDot),
            ("PRN.", TrailingDot),
            ("book.usfm ", TrailingSpace),
            ("ingredients/book.usfm ", TrailingSpace),
            ("dir /x", TrailingSpace),
            (". ", TrailingSpace),
            (" ", TrailingSpace),
            ("CON", ReservedName),
            ("con.txt", ReservedName),
            ("con .txt", ReservedName),
            ("x/Aux.tsv", ReservedName),
            ("lpt9.md", ReservedName),
            ("COM\u{b9}", ReservedName),
            ("conout$.txt", ReservedName),
            ("PROGRA~1", ShortName),
            ("x/abcdef~1.tsv", ShortName),
            ("A~9", ShortName),
        ] {
            assert_eq!(check_path(path), Err(problem), "{path:?}");
        }
    }

    #[test]
    fn a_problem_that_leaves_the_folder_is_reported_first() {
        for path in ["a<b/../x", "con/../x", "a /..", "x\u{1}/./y", "a:b\\c"] {
            let problem = check_path(path).unwrap_err();
            assert!(problem.leaves_folder(), "{path:?} {problem:?}");
        }
    }

    #[test]
    fn ordinary_names_pass() {
        for path in [
            "ingredients/TIT.usfm",
            ".gitignore",
            ".tc/packs/x.pack",
            "LICENSE copy.md",
            "my file/a b.md",
            "a/..b",
            "a/b..c",
            "Jonás.usfm",
            "CONTENT.md",
            "com10",
            "console/x",
            "notes~1.json",
            "notes~2024-draft.md",
            "abcdefg~1",
            "~1",
            "a~b",
        ] {
            assert_eq!(check_path(path), Ok(()), "{path:?}");
        }
    }
}
