//! Story Markdown (tC4 `src/data/export/obsMarkdown.ts`, #360, J23): an OBS project's
//! `ingredients/content/` files, byte for byte, as `content/…`, without a `.bak` file or a
//! `.DS_Store` (as the Scripture Burrito zip leaves them out). The app zips them.

const INGREDIENTS: &str = "ingredients/";
const CONTENT: &str = "ingredients/content/";

/// The `content/` files of `files`, the project's files by their path from the project
/// folder, in the order given.
pub fn content_files(files: &[(String, Vec<u8>)]) -> Vec<(String, Vec<u8>)> {
    files
        .iter()
        .filter(|(path, _)| path.starts_with(CONTENT) && !path.ends_with('/') && !excluded(path))
        .filter_map(|(path, bytes)| {
            Some((path.strip_prefix(INGREDIENTS)?.to_owned(), bytes.clone()))
        })
        .collect()
}

/// tC4's `excluded` (`burritoZip.ts`) under `content/`, where no `.git` folder stands.
fn excluded(path: &str) -> bool {
    path.rsplit('/').next() == Some(".DS_Store") || path.ends_with(".bak")
}
