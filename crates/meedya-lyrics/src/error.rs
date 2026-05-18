pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("lyrics not found")]
    NotFound,
    #[error("malformed LRC: {0}")]
    Lrc(String),
    #[error("metadata: {0}")]
    Metadata(#[from] meedya_metadata::MetadataError),
    #[error("synced lyrics requested but none present")]
    NoSyncedLyrics,
    #[error("container does not support synchronised lyrics (SYLT is ID3v2-only; got {tag_type})")]
    UnsupportedForSync { tag_type: String },
    #[error("invalid ISO-639-2 language code (must be 3 ASCII letters)")]
    InvalidLanguageCode,
}
