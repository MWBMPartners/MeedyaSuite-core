use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, ItemValue, Tag, TagItem, TagType};
use std::collections::HashMap;
use std::path::Path;

use crate::error::TagError;

/// Audio properties extracted from a file.
#[derive(Debug, Clone)]
pub struct AudioProperties {
    pub duration_ms: u64,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub channels: Option<u8>,
    pub bitrate: Option<u32>,
}

/// Extract metadata tags from an audio file as a flat key-value map.
pub fn extract_tags(path: &Path) -> Result<HashMap<String, String>, TagError> {
    let tagged_file = Probe::open(path)
        .map_err(|e| TagError::ReadFailed(e.to_string()))?
        .read()
        .map_err(|e| TagError::ReadFailed(e.to_string()))?;

    let mut tags = HashMap::new();

    if let Some(tag) = tagged_file.primary_tag().or(tagged_file.first_tag()) {
        if let Some(v) = tag.title() {
            tags.insert("title".to_string(), v.to_string());
        }
        if let Some(v) = tag.artist() {
            tags.insert("artist".to_string(), v.to_string());
        }
        if let Some(v) = tag.album() {
            tags.insert("album".to_string(), v.to_string());
        }
        if let Some(v) = tag.genre() {
            tags.insert("genre".to_string(), v.to_string());
        }
        if let Some(v) = tag.year() {
            tags.insert("year".to_string(), v.to_string());
        }
        if let Some(v) = tag.track() {
            tags.insert("track_number".to_string(), v.to_string());
        }
        if let Some(v) = tag.disk() {
            tags.insert("disc_number".to_string(), v.to_string());
        }
        if let Some(v) = tag.comment() {
            tags.insert("comment".to_string(), v.to_string());
        }

        // Collect all other items
        for item in tag.items() {
            let key = format!("{:?}", item.key());
            if let ItemValue::Text(text) = item.value() {
                tags.entry(key).or_insert_with(|| text.clone());
            }
        }
    }

    Ok(tags)
}

/// Extract audio properties (duration, sample rate, etc.) from a file.
pub fn extract_audio_properties(path: &Path) -> Result<AudioProperties, TagError> {
    let tagged_file = Probe::open(path)
        .map_err(|e| TagError::ReadFailed(e.to_string()))?
        .read()
        .map_err(|e| TagError::ReadFailed(e.to_string()))?;

    let props = tagged_file.properties();

    Ok(AudioProperties {
        duration_ms: props.duration().as_millis() as u64,
        sample_rate: props.sample_rate(),
        bit_depth: props.bit_depth(),
        channels: props.channels(),
        bitrate: props.overall_bitrate(),
    })
}

/// Extract cover art from a file, if present.
pub fn extract_cover_art(path: &Path) -> Result<Option<Vec<u8>>, TagError> {
    let tagged_file = Probe::open(path)
        .map_err(|e| TagError::ReadFailed(e.to_string()))?
        .read()
        .map_err(|e| TagError::ReadFailed(e.to_string()))?;

    if let Some(tag) = tagged_file.primary_tag().or(tagged_file.first_tag()) {
        if let Some(picture) = tag.pictures().first() {
            return Ok(Some(picture.data().to_vec()));
        }
    }

    Ok(None)
}

/// Write metadata tags to an audio file.
///
/// Tags are provided as key-value pairs. Standard keys (title, artist, album, etc.)
/// are mapped to format-specific fields automatically.
pub fn write_tags(path: &Path, tags: &HashMap<String, String>) -> Result<(), TagError> {
    let mut tagged_file = Probe::open(path)
        .map_err(|e| TagError::WriteFailed(e.to_string()))?
        .read()
        .map_err(|e| TagError::WriteFailed(e.to_string()))?;

    let tag_type = tagged_file
        .primary_tag()
        .map(|t| t.tag_type())
        .unwrap_or(TagType::Id3v2);

    if tagged_file.tag_mut(tag_type).is_none() {
        tagged_file.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged_file.tag_mut(tag_type).unwrap();

    for (key, value) in tags {
        match key.as_str() {
            "title" => tag.set_title(value.to_string()),
            "artist" => tag.set_artist(value.to_string()),
            "album" => tag.set_album(value.to_string()),
            "genre" => tag.set_genre(value.to_string()),
            "comment" => tag.set_comment(value.to_string()),
            "year" => {
                if let Ok(y) = value.parse::<u32>() {
                    tag.set_year(y);
                }
            }
            "track_number" => {
                if let Ok(t) = value.parse::<u32>() {
                    tag.set_track(t);
                }
            }
            "disc_number" => {
                if let Ok(d) = value.parse::<u32>() {
                    tag.set_disk(d);
                }
            }
            other => {
                // Try to map to a known ItemKey, otherwise use unknown
                let item_key = string_to_item_key(other);
                let item = TagItem::new(item_key, ItemValue::Text(value.clone()));
                tag.push(item);
            }
        }
    }

    tagged_file
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| TagError::WriteFailed(e.to_string()))?;

    Ok(())
}

/// Remove a specific tag key from a file.
pub fn remove_tag(path: &Path, key: &str) -> Result<(), TagError> {
    let mut tagged_file = Probe::open(path)
        .map_err(|e| TagError::WriteFailed(e.to_string()))?
        .read()
        .map_err(|e| TagError::WriteFailed(e.to_string()))?;

    if let Some(tag) = tagged_file.primary_tag_mut() {
        let item_key = string_to_item_key(key);
        tag.remove_key(&item_key);

        tagged_file
            .save_to_path(path, WriteOptions::default())
            .map_err(|e| TagError::WriteFailed(e.to_string()))?;
    }

    Ok(())
}

/// Embed cover art into a file.
pub fn embed_cover_art(
    path: &Path,
    data: Vec<u8>,
    mime_type: lofty::picture::MimeType,
) -> Result<(), TagError> {
    let mut tagged_file = Probe::open(path)
        .map_err(|e| TagError::WriteFailed(e.to_string()))?
        .read()
        .map_err(|e| TagError::WriteFailed(e.to_string()))?;

    let tag_type = tagged_file
        .primary_tag()
        .map(|t| t.tag_type())
        .unwrap_or(TagType::Id3v2);

    if tagged_file.tag_mut(tag_type).is_none() {
        tagged_file.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged_file.tag_mut(tag_type).unwrap();

    let picture = lofty::picture::Picture::new_unchecked(
        lofty::picture::PictureType::CoverFront,
        Some(mime_type),
        None,
        data,
    );
    tag.push_picture(picture);

    tagged_file
        .save_to_path(path, WriteOptions::default())
        .map_err(|e| TagError::WriteFailed(e.to_string()))?;

    Ok(())
}

fn string_to_item_key(key: &str) -> ItemKey {
    match key {
        "isrc" => ItemKey::Isrc,
        "composer" => ItemKey::Composer,
        "conductor" => ItemKey::Conductor,
        "encoder" => ItemKey::EncoderSoftware,
        "copyright" => ItemKey::CopyrightMessage,
        "lyrics" => ItemKey::Lyrics,
        "album_artist" => ItemKey::AlbumArtist,
        "release_date" => ItemKey::RecordingDate,
        "record_label" => ItemKey::Label,
        "title_sort" => ItemKey::Unknown("TITLESORTORDER".to_string()),
        "artist_sort" => ItemKey::Unknown("ARTISTSORTORDER".to_string()),
        "album_sort" => ItemKey::Unknown("ALBUMSORTORDER".to_string()),
        "album_artist_sort" => ItemKey::AlbumArtistSortOrder,
        "composer_sort" => ItemKey::Unknown("COMPOSERSORTORDER".to_string()),
        "work" => ItemKey::Work,
        "movement" => ItemKey::MovementNumber,
        "movement_number" => ItemKey::MovementNumber,
        other => ItemKey::Unknown(other.to_string()),
    }
}
