// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License.

use thiserror::Error;

/// Errors that can occur in metadata operations.
///
/// **`#[non_exhaustive]`** (from the stand-in review of revision 5,
/// policy MWBM-MEDIA-LANG): a `match` on this enum in another crate needs
/// a `_ =>` arm. Marked now because the enum was already changing — the
/// `UnrecognisedLanguage` variant was added after Codex's review r7, which
/// on its own broke any caller that matched every variant — so callers
/// take one break, not one per future variant: from here on, adding a
/// variant does not break them. (Matches inside this crate are not
/// affected by the attribute.)
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MetadataError {
    #[error("failed to parse tag registry TOML: {0}")]
    RegistryParseError(String),

    #[error("unknown value_type '{value_type}' for tag '{tag_id}'")]
    UnknownValueType { tag_id: String, value_type: String },

    #[error("unknown namespace '{namespace}' for tag '{tag_id}' (expected one of: {expected})")]
    UnknownNamespace {
        tag_id: String,
        namespace: String,
        expected: String,
    },

    #[error("JSON path extraction failed for path '{path}'")]
    PathExtractionFailed { path: String },

    #[error("value conversion failed for tag '{tag_id}': {reason}")]
    ValueConversionFailed { tag_id: String, reason: String },

    /// A `CommonTag::Language` value given to `write_tags` that the
    /// language reader (policy MWBM-MEDIA-LANG, LANG-002) does not
    /// recognise — `zzz`, a language *name* such as `English`, a locale
    /// name such as `en_GB`, or nothing at all. The write is refused as a
    /// whole and nothing is written to the file: storing the text would
    /// put something that is not a language into a language field, and
    /// storing `und` in its place would quietly lose what the caller said
    /// (LANG-002, COMPAT-040 — report doubt, never resolve it by guessing).
    ///
    /// `value` is the whole value exactly as given; `problem` says which
    /// part was not recognised (for a value holding several languages,
    /// which one of them).
    #[error(
        "cannot write the language {value:?}: {problem}. Give a BCP 47 language tag such as \
         `en`, `pt-BR` or `zh-Hant` (an old three-letter code such as `eng` is also accepted), \
         or `und` when the language is not known. Nothing was written to the file"
    )]
    UnrecognisedLanguage { value: String, problem: String },

    // --- File I/O errors ---
    #[error("file not found: {0}")]
    FileNotFound(String),

    #[error("unsupported file format: {0}")]
    UnsupportedFormat(String),

    #[error("failed to read tags from file: {0}")]
    ReadError(String),

    #[error("failed to write tags to file: {0}")]
    WriteError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("lofty error: {0}")]
    LoftyError(#[from] lofty::error::LoftyError),
}
