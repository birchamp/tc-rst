//! Which picture a frame shows (tC4 `src/data/obsImages.ts`, #288, D75).
//!
//! A frame's image line names its picture by URL, such as
//! `https://cdn.door43.org/obs/jpg/360px/obs-en-01-02.jpg`. The picture is looked up by the
//! URL's file name (`obs-en-01-02.jpg`) in the project's own picture ingredients, then in
//! the picture packs the project pins (primary, then fallback), then in the default pack,
//! `uW/obs_images_360`, whose files sit at `360px/obs-en-NN-FF.jpg`. Selection only reads:
//! the image line stays byte for byte, and the screen shows the file it found.

use std::collections::BTreeMap;

use url::Url;

use crate::js::{decode_uri_component, is_space, utf16_cmp};

/// A picture pack's identity: its Door43 repository path and the commit it is pinned at.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackPin {
    /// Such as `git.door43.org/uW/obs_images_360`.
    pub repo_path: String,
    pub sha: String,
}

impl PackPin {
    /// The same pack: the repository without regard to case, the commit exactly
    /// (`resolveObsImage`, `obsImages.ts:138-139`).
    fn is(&self, other: &PackPin) -> bool {
        self.sha == other.sha && self.repo_path.to_lowercase() == other.repo_path.to_lowercase()
    }
}

/// The default picture pack's repository and commit (`DEFAULT_OBS_IMAGES`,
/// `obsImages.ts:6-10`). The release pipeline fetches it at this commit; tc-resources
/// ships the same pin as `suite::obs_images()`.
pub const DEFAULT_PACK_REPO: &str = "git.door43.org/uW/obs_images_360";
pub const DEFAULT_PACK_SHA: &str = "7146d5b504f6b63b9e11f7dc0b18c594d0ae179d";

pub fn default_pack_pin() -> PackPin {
    PackPin {
        repo_path: DEFAULT_PACK_REPO.to_owned(),
        sha: DEFAULT_PACK_SHA.to_owned(),
    }
}

/// The `obs-images` pins of a project's two language sets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImagePins {
    pub primary: Option<PackPin>,
    pub fallback: Option<PackPin>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rung {
    Primary,
    Fallback,
}

impl ImagePins {
    fn rungs(&self) -> [(Rung, Option<&PackPin>); 2] {
        [
            (Rung::Primary, self.primary.as_ref()),
            (Rung::Fallback, self.fallback.as_ref()),
        ]
    }
}

/// A picture the renderer can read (`ObsImageAsset`, `obsImages.ts:12-20`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageAsset {
    /// A path or URL the renderer reads.
    pub uri: String,
    /// The burrito ingredient role; a project ingredient counts only as `x-obsimages`.
    pub role: Option<String>,
    /// `Some(false)` when a loader could not decode the bytes; the candidate then falls
    /// through to the next rung.
    pub decodable: Option<bool>,
}

impl ImageAsset {
    pub fn at(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            role: None,
            decodable: None,
        }
    }

    fn usable_uri(&self) -> Option<&str> {
        (self.decodable != Some(false)).then_some(self.uri.as_str())
    }
}

/// The role of OBS picture ingredients.
pub const IMAGE_ROLE: &str = "x-obsimages";

/// A picture pack's files by name. A name with more than one file is ambiguous, and so
/// unusable (`ObsImagePack`, `obsImages.ts:22-28`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePack {
    pub pin: PackPin,
    pub images: BTreeMap<String, Vec<ImageAsset>>,
}

impl ImagePack {
    pub fn empty(pin: PackPin) -> Self {
        Self {
            pin,
            images: BTreeMap::new(),
        }
    }

    /// How many picture names the pack holds.
    pub fn files(&self) -> usize {
        self.images.len()
    }
}

fn is_picture_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".jpg", ".jpeg", ".png", ".webp"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// A pack's files by name, from the pack's paths below `ingredients/` — the real file
/// listing, which holds files no ingredient table lists. Sorted first, so the result never
/// depends on the listing's order (`obsImagePackFromPaths`, `obsImages.ts:34-46`).
pub fn pack_from_paths<S: AsRef<str>>(
    pin: PackPin,
    paths: &[S],
    uri_for: impl Fn(&str) -> String,
) -> ImagePack {
    let mut sorted: Vec<&str> = paths.iter().map(AsRef::as_ref).collect();
    sorted.sort_by(|a, b| utf16_cmp(a, b));
    let mut images: BTreeMap<String, Vec<ImageAsset>> = BTreeMap::new();
    for ipath in sorted {
        let name = ipath.rsplit('/').next().unwrap_or(ipath);
        if !is_picture_name(name) {
            continue;
        }
        images.entry(name.to_owned()).or_default().push(ImageAsset {
            uri: uri_for(ipath),
            role: Some(IMAGE_ROLE.to_owned()),
            decodable: None,
        });
    }
    ImagePack { pin, images }
}

/// The same from a pack's `metadata.json` ingredient keys, for when the listing cannot be
/// read: the table is only as complete as the last rescan that wrote it
/// (`obsImagePackFromMetadata`, `obsImages.ts:53-62`).
pub fn pack_from_ingredients<'a>(
    pin: PackPin,
    ingredient_keys: impl IntoIterator<Item = &'a str>,
    uri_for: impl Fn(&str) -> String,
) -> ImagePack {
    let paths: Vec<&str> = ingredient_keys
        .into_iter()
        .filter_map(|key| key.strip_prefix("ingredients/"))
        .collect();
    pack_from_paths(pin, &paths, uri_for)
}

/// The project's own picture ingredients: those with role `x-obsimages` below
/// `ingredients/` (`projectImageIngredients`, `src/data/obsStory.ts:147-159`). `uri_for`
/// gets the path below `ingredients/`.
pub fn project_image_assets<'a>(
    ingredients: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    uri_for: impl Fn(&str) -> String,
) -> Vec<ImageAsset> {
    ingredients
        .into_iter()
        .filter(|(_, role)| *role == Some(IMAGE_ROLE))
        .filter_map(|(key, _)| key.strip_prefix("ingredients/"))
        .map(|ipath| ImageAsset {
            uri: uri_for(ipath),
            role: Some(IMAGE_ROLE.to_owned()),
            decodable: None,
        })
        .collect()
}

/// Where a frame's picture comes from (`ObsImageResolution`, `obsImages.ts:64-68`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageResolution {
    Project {
        uri: String,
        file_name: String,
    },
    Pin {
        uri: String,
        file_name: String,
        pin: PackPin,
        rung: Rung,
    },
    Default {
        uri: String,
        file_name: String,
        pin: PackPin,
    },
    Missing {
        file_name: Option<String>,
    },
}

impl ImageResolution {
    pub fn uri(&self) -> Option<&str> {
        match self {
            Self::Project { uri, .. } | Self::Pin { uri, .. } | Self::Default { uri, .. } => {
                Some(uri)
            }
            Self::Missing { .. } => None,
        }
    }
}

const URL_BASE: &str = "https://local.invalid/";

fn join(input: &str) -> Option<Url> {
    Url::parse(URL_BASE).ok()?.join(input).ok()
}

/// The last `/` segment of a URL path, percent-decoded; `None` when empty or undecodable.
fn last_segment(path: &str) -> Option<String> {
    let segment = path.rsplit('/').next().unwrap_or(path);
    decode_uri_component(segment).filter(|name| !name.is_empty())
}

/// The picture file name an image line names (`obsImageFileName`, `obsImages.ts:71-80`):
/// the line must be `![alt](url)`, optionally with a quoted title after the URL, and the
/// name is the URL path's last segment, percent-decoded.
pub fn image_file_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("![")?;
    let rest = &rest[rest.find(']')? + 1..];
    let rest = rest.strip_prefix('(')?;
    let url_len = rest
        .find(|c: char| is_space(c) || c == ')')
        .unwrap_or(rest.len());
    if url_len == 0 {
        return None;
    }
    let (url, mut rest) = rest.split_at(url_len);
    if !rest.starts_with(')') {
        // `\s+['"][^'"]*['"]`: a title in quotes, which may differ.
        rest = rest.trim_start_matches(is_space);
        rest = rest.strip_prefix(['\'', '"'])?;
        rest = &rest[rest.find(['\'', '"'])? + 1..];
    }
    let after = rest.strip_prefix(')')?;
    if !after.chars().all(is_space) {
        return None;
    }
    last_segment(join(url)?.path())
}

/// The file name of a project ingredient's URI (`basename`, `obsImages.ts:101-116`). The
/// app serves an ingredient at a URL whose query `ipath` names the file, so that query wins
/// over the path; it is decoded once, by the query parser.
fn uri_file_name(uri: &str) -> Option<String> {
    let url = join(uri)?;
    let ipath = url
        .query_pairs()
        .find(|(key, _)| key == "ipath")
        .map(|(_, value)| value.into_owned());
    if let Some(ipath) = ipath.filter(|p| !p.is_empty()) {
        let name = ipath.rsplit('/').next().unwrap_or(&ipath);
        return (!name.is_empty()).then(|| name.to_owned());
    }
    let path = url.path();
    let path = if path.is_empty() || path == "/" {
        url.host_str().unwrap_or_default()
    } else {
        path
    };
    last_segment(path)
}

fn one_candidate(candidates: Option<&Vec<ImageAsset>>) -> Option<&str> {
    match candidates.map(Vec::as_slice) {
        Some([only]) => only.usable_uri(),
        _ => None,
    }
}

/// Resolves a frame's picture at render time: the project's own ingredient, then the
/// primary pin's pack, then the fallback pin's pack, then the default pack. A missing,
/// ambiguous or undecodable candidate falls through (`resolveObsImage`,
/// `obsImages.ts:120-148`). `packs` are the pinned packs read; the first whose pin is the
/// rung's answers for it.
pub fn resolve_image(
    image_line: &str,
    pins: &ImagePins,
    project: &[ImageAsset],
    packs: &[ImagePack],
    default_pack: &ImagePack,
) -> ImageResolution {
    let Some(file_name) = image_file_name(image_line) else {
        return ImageResolution::Missing { file_name: None };
    };
    let matches: Vec<&ImageAsset> = project
        .iter()
        .filter(|asset| {
            asset.role.as_deref() == Some(IMAGE_ROLE)
                && uri_file_name(&asset.uri).as_deref() == Some(file_name.as_str())
        })
        .collect();
    if let [only] = matches.as_slice()
        && let Some(uri) = only.usable_uri()
    {
        return ImageResolution::Project {
            uri: uri.to_owned(),
            file_name,
        };
    }
    for (rung, pin) in pins.rungs() {
        let Some(pin) = pin else { continue };
        let pack = packs.iter().find(|pack| pack.pin.is(pin));
        if let Some(uri) = one_candidate(pack.and_then(|p| p.images.get(&file_name))) {
            return ImageResolution::Pin {
                uri: uri.to_owned(),
                file_name,
                pin: pin.clone(),
                rung,
            };
        }
    }
    match one_candidate(default_pack.images.get(&file_name)) {
        Some(uri) => ImageResolution::Default {
            uri: uri.to_owned(),
            file_name,
            pin: default_pack.pin.clone(),
        },
        None => ImageResolution::Missing {
            file_name: Some(file_name),
        },
    }
}
