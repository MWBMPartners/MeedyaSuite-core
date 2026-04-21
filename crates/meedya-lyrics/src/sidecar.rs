//! Write lyrics to a `.lrc` sidecar next to a media file.

use std::path::{Path, PathBuf};

use crate::{lrc, Lyrics, Result};

pub fn sidecar_path_for(media: &Path) -> PathBuf {
    media.with_extension("lrc")
}

/// Writes an LRC sidecar if synced lyrics are present.
///
/// Returns the written path, or `None` if `lyrics` has no synced content
/// (LRC is inherently timestamped — plain-only lyrics should go to tag embed
/// or a different sidecar format).
pub fn write(media: &Path, lyrics: &Lyrics) -> Result<Option<PathBuf>> {
    let Some(synced) = lyrics.synced.as_deref() else {
        return Ok(None);
    };
    if synced.is_empty() {
        return Ok(None);
    }
    let path = sidecar_path_for(media);
    std::fs::write(&path, lrc::write(synced))?;
    Ok(Some(path))
}
