use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// An empty setting follows the desktop's Pictures location, including future
/// changes to it. Custom paths are validated at the serialization/UI boundary.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ScreenshotDirectory {
    #[default]
    Pictures,
    Custom(AbsoluteDirectory),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbsoluteDirectory(String);

impl TryFrom<String> for ScreenshotDirectory {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Ok(Self::Pictures)
        } else if Path::new(&value).is_absolute() && !value.contains('\0') {
            Ok(Self::Custom(AbsoluteDirectory(value)))
        } else {
            Err("screenshot_directory must be an absolute directory path")
        }
    }
}

impl From<ScreenshotDirectory> for String {
    fn from(value: ScreenshotDirectory) -> Self {
        match value {
            ScreenshotDirectory::Pictures => String::new(),
            ScreenshotDirectory::Custom(path) => path.0,
        }
    }
}

impl ScreenshotDirectory {
    pub fn resolve(&self, pictures: &Path) -> Option<PathBuf> {
        match self {
            Self::Pictures => pictures
                .is_absolute()
                .then(|| pictures.join("mirakurun-viewer")),
            Self::Custom(path) => Some(PathBuf::from(&path.0)),
        }
    }
}
