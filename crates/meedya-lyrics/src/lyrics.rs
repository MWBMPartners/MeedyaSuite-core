use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedLine {
    pub at: Duration,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lyrics {
    pub plain: Option<String>,
    pub synced: Option<Vec<SyncedLine>>,
}

impl Lyrics {
    pub fn is_empty(&self) -> bool {
        self.plain.as_deref().is_none_or(str::is_empty)
            && self.synced.as_ref().is_none_or(|s| s.is_empty())
    }
}
