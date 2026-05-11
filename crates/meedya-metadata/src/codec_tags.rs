// Copyright (c) 2024-2026 MWBM Partners Ltd
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Codec-specific metadata tags for M4A files.
//
// These tags identify the audio codec variant used at download time.
// They are written as MP4 freeform atoms in dual namespaces.

use mp4ameta::{Data, FreeformIdent, Tag};

use crate::registry::{ITUNES_NAMESPACE, MEEDYA_NAMESPACE};

/// Audio codec variant for metadata tagging purposes.
///
/// This is a subset of codec types that affect which metadata tags are
/// written. Downstream crates (e.g., `meedya-codecs`) define the full
/// codec enum — this enum covers only what the tag writer needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecKind {
    /// Apple Lossless Audio Codec.
    Lossless,
    /// Dolby Atmos spatial audio (E-AC-3 JOC).
    Atmos,
    /// Dolby Digital surround (AC-3).
    DolbyDigital,
    /// Binaural HRTF rendering (AAC or AAC-HE).
    Binaural,
    /// Stereo downmix from surround (AAC or AAC-HE).
    Downmix,
    /// Standard lossy codec (AAC, AAC-HE, etc.) — no special tags.
    StandardLossy,
}

impl CodecKind {
    /// Returns the CLI string identifier for this codec kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lossless => "alac",
            Self::Atmos => "atmos",
            Self::DolbyDigital => "ac3",
            Self::Binaural => "aac-binaural",
            Self::Downmix => "aac-downmix",
            Self::StandardLossy => "aac",
        }
    }
}

/// Apply codec-specific metadata tags to all M4A files at the given path.
///
/// Walks the path (file or directory, recursively) and tags every `.m4a`
/// file with codec identification atoms.
///
/// Returns the number of files successfully tagged.
pub fn apply_codec_metadata_tags(output_path: &str, codec: &CodecKind) -> Result<usize, String> {
    let tag_writer: Box<dyn Fn(&mut Tag)> = match codec {
        CodecKind::Lossless => Box::new(|tag: &mut Tag| {
            write_lossless_tags(tag);
            clear_binaural_downmix_tags(tag);
        }),
        CodecKind::Atmos => Box::new(|tag: &mut Tag| {
            write_atmos_tags(tag);
            clear_binaural_downmix_tags(tag);
        }),
        CodecKind::DolbyDigital => Box::new(|tag: &mut Tag| {
            write_dolby_digital_tags(tag);
            clear_binaural_downmix_tags(tag);
        }),
        CodecKind::Binaural => Box::new(write_binaural_tags),
        CodecKind::Downmix => Box::new(write_downmix_tags),
        CodecKind::StandardLossy => Box::new(clear_binaural_downmix_tags),
    };

    let path = std::path::Path::new(output_path);
    let mut tagged_count = 0;

    if path.is_file() {
        if crate::writer::is_m4a(path) {
            match crate::writer::tag_single_file(path, &tag_writer) {
                Ok(()) => tagged_count += 1,
                Err(e) => {
                    log::debug!("Failed to tag {}: {}", path.display(), e);
                }
            }
        }
    } else if path.is_dir() {
        tagged_count += crate::writer::tag_directory_recursive(path, &tag_writer);
    } else {
        return Err(format!("Output path does not exist: {output_path}"));
    }

    Ok(tagged_count)
}

// ============================================================
// Codec Tag Writers
// ============================================================

/// Writes lossless (ALAC) identification tags.
///
/// Tags written: `----:com.apple.iTunes:isLossless` → "Y"
pub fn write_lossless_tags(tag: &mut Tag) {
    let ident = FreeformIdent::new_static(ITUNES_NAMESPACE, "isLossless");
    tag.set_data(ident, Data::Utf8("Y".to_owned()));
}

/// Writes Dolby Atmos spatial audio identification tags.
///
/// Tags written:
///   - `----:com.apple.iTunes:SpatialType` → "Dolby Atmos"
///   - `----:MeedyaMeta:SpatialType`       → "Dolby Atmos"
pub fn write_atmos_tags(tag: &mut Tag) {
    let itunes_ident = FreeformIdent::new_static(ITUNES_NAMESPACE, "SpatialType");
    tag.set_data(itunes_ident, Data::Utf8("Dolby Atmos".to_owned()));

    let meedya_ident = FreeformIdent::new_static(MEEDYA_NAMESPACE, "SpatialType");
    tag.set_data(meedya_ident, Data::Utf8("Dolby Atmos".to_owned()));
}

/// Writes Dolby Digital (AC-3) surround audio identification tags.
///
/// Tags written:
///   - `----:com.apple.iTunes:SpatialType` → "Dolby Digital"
///   - `----:MeedyaMeta:SpatialType`       → "Dolby Digital"
pub fn write_dolby_digital_tags(tag: &mut Tag) {
    let itunes_ident = FreeformIdent::new_static(ITUNES_NAMESPACE, "SpatialType");
    tag.set_data(itunes_ident, Data::Utf8("Dolby Digital".to_owned()));

    let meedya_ident = FreeformIdent::new_static(MEEDYA_NAMESPACE, "SpatialType");
    tag.set_data(meedya_ident, Data::Utf8("Dolby Digital".to_owned()));
}

/// Writes binaural audio identification tags.
///
/// Tags written:
///   - `----:com.apple.iTunes:isBinaural` → "Y"
///   - `----:MeedyaMeta:isBinaural`       → "Y"
pub fn write_binaural_tags(tag: &mut Tag) {
    tag.set_data(
        FreeformIdent::new_static(ITUNES_NAMESPACE, "isBinaural"),
        Data::Utf8("Y".to_owned()),
    );
    tag.set_data(
        FreeformIdent::new_static(MEEDYA_NAMESPACE, "isBinaural"),
        Data::Utf8("Y".to_owned()),
    );
}

/// Writes downmix audio identification tags.
///
/// Tags written:
///   - `----:com.apple.iTunes:isDownmix` → "Y"
///   - `----:MeedyaMeta:isDownmix`       → "Y"
pub fn write_downmix_tags(tag: &mut Tag) {
    tag.set_data(
        FreeformIdent::new_static(ITUNES_NAMESPACE, "isDownmix"),
        Data::Utf8("Y".to_owned()),
    );
    tag.set_data(
        FreeformIdent::new_static(MEEDYA_NAMESPACE, "isDownmix"),
        Data::Utf8("Y".to_owned()),
    );
}

/// Removes isBinaural and isDownmix tags from an M4A file's metadata.
///
/// Apple Music's servers may embed delivery-mode indicators regardless
/// of which codec was actually downloaded. This strips them when the
/// effective codec is not binaural or downmix.
pub fn clear_binaural_downmix_tags(tag: &mut Tag) {
    let binaural_itunes = FreeformIdent::new_static(ITUNES_NAMESPACE, "isBinaural");
    let binaural_meedya = FreeformIdent::new_static(MEEDYA_NAMESPACE, "isBinaural");
    let downmix_itunes = FreeformIdent::new_static(ITUNES_NAMESPACE, "isDownmix");
    let downmix_meedya = FreeformIdent::new_static(MEEDYA_NAMESPACE, "isDownmix");

    let had_binaural = tag.strings_of(&binaural_itunes).next().is_some()
        || tag.strings_of(&binaural_meedya).next().is_some();
    let had_downmix = tag.strings_of(&downmix_itunes).next().is_some()
        || tag.strings_of(&downmix_meedya).next().is_some();

    tag.remove_data_of(&binaural_itunes);
    tag.remove_data_of(&binaural_meedya);
    tag.remove_data_of(&downmix_itunes);
    tag.remove_data_of(&downmix_meedya);

    if had_binaural || had_downmix {
        log::debug!(
            "Cleared inherited binaural/downmix tags (binaural={had_binaural}, downmix={had_downmix})"
        );
    }
}

/// Write the spatial audio codec identifier for downstream ISRC matching.
///
/// When the detected codec is spatial (Atmos, AC3, Binaural), tags the
/// file with `MeedyaMeta:SpatialAudioCodec` so downstream tools know
/// this ISRC is from the spatial version of the track.
pub fn write_spatial_codec_tag(tag: &mut Tag, codec: &CodecKind) {
    let spatial_codecs = [CodecKind::Atmos, CodecKind::DolbyDigital, CodecKind::Binaural];
    if spatial_codecs.contains(codec) {
        let ident = FreeformIdent::new_static(MEEDYA_NAMESPACE, "SpatialAudioCodec");
        tag.set_data(ident, Data::Utf8(codec.as_str().to_string()));
    }
}
