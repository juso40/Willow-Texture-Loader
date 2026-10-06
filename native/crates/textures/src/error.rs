//! Crate-wide error type.
//!
//! Every failure carries one categorized variant with its full
//! human-readable message as the payload.

use std::fmt;

use unrealsdk_rs::objects::Error as ueError;

/// The crate's error type: a category plus its full message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureError {
    /// A named object, class, package or property does not exist.
    NotFound(String),
    /// The object exists but is not the type the operation needs.
    NotATexture(String),
    /// Caller input missing or unusable (no name, no target, no free name).
    InvalidInput(String),
    /// The mip chain handed to a writer is empty.
    NoMips,
    /// The SDK offset table is unavailable (off-game or not initialized).
    OffsetsUnavailable,
    /// The `Mips` field layout is not the writer's known form.
    UnsupportedLayout(String),
    /// A pixel format the export path cannot decode.
    UnsupportedFormat(String),
    /// Data failed a plausibility guard (corrupt or foreign layout).
    Implausible(String),
    /// File read failure.
    Io(String),
    /// Image decode failure.
    Image(String),
    /// Engine call or engine-heap allocation failed.
    Engine(String),
    /// Console command registration failure.
    Command(String),
}

impl TextureError {
    /// The full human-readable message.
    pub fn message(&self) -> &str {
        match self {
            Self::NotFound(m)
            | Self::NotATexture(m)
            | Self::InvalidInput(m)
            | Self::UnsupportedLayout(m)
            | Self::UnsupportedFormat(m)
            | Self::Implausible(m)
            | Self::Io(m)
            | Self::Image(m)
            | Self::Engine(m)
            | Self::Command(m) => m,
            Self::NoMips => "no mips to upload",
            Self::OffsetsUnavailable => "offsets unavailable",
        }
    }
}

impl fmt::Display for TextureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for TextureError {}

impl From<ueError> for TextureError {
    fn from(e: ueError) -> Self {
        match e {
            ueError::NotInGame => Self::OffsetsUnavailable,
            ueError::NullObject | ueError::NotFound(_) => Self::NotFound(e.to_string()),
            ueError::TypeMismatch { .. }
            | ueError::Unsupported(_)
            | ueError::ArgCount { .. }
            | ueError::AllocFailed => Self::Engine(e.to_string()),
            ueError::Duplicate(_) | ueError::EmptyName(_) | ueError::Failed(_) => {
                Self::Command(e.to_string())
            }
        }
    }
}
