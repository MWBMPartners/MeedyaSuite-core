use crate::types::CoverArtInfo;

/// Size classification for cover art.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CoverArtSize {
    Unknown,
    Thumbnail,  // < 200px
    Small,      // 200–499px
    Medium,     // 500–999px
    Large,      // 1000–1999px
    ExtraLarge, // >= 2000px
}

impl CoverArtSize {
    /// Classify a dimension (width or height) into a size bucket.
    pub fn from_dimension(px: u32) -> Self {
        match px {
            0 => Self::Unknown,
            1..=199 => Self::Thumbnail,
            200..=499 => Self::Small,
            500..=999 => Self::Medium,
            1000..=1999 => Self::Large,
            _ => Self::ExtraLarge,
        }
    }
}

/// Classify a cover art image by its largest dimension.
pub fn classify(art: &CoverArtInfo) -> CoverArtSize {
    let max_dim = art.width.unwrap_or(0).max(art.height.unwrap_or(0));
    CoverArtSize::from_dimension(max_dim)
}

/// Select the largest cover art from a list.
pub fn select_largest(arts: &[CoverArtInfo]) -> Option<&CoverArtInfo> {
    arts.iter().max_by_key(|a| {
        a.width.unwrap_or(0).max(a.height.unwrap_or(0))
    })
}

/// Select the smallest cover art from a list.
pub fn select_smallest(arts: &[CoverArtInfo]) -> Option<&CoverArtInfo> {
    if arts.is_empty() {
        return None;
    }
    arts.iter().min_by_key(|a| {
        let dim = a.width.unwrap_or(0).max(a.height.unwrap_or(0));
        if dim == 0 { u32::MAX } else { dim }
    })
}

/// Select the best cover art meeting a minimum size threshold.
pub fn select_best(arts: &[CoverArtInfo], min_size: CoverArtSize) -> Option<&CoverArtInfo> {
    arts.iter()
        .filter(|a| classify(a) >= min_size)
        .min_by_key(|a| a.width.unwrap_or(0).max(a.height.unwrap_or(0)))
}

/// Filter cover arts to only those meeting a minimum size, sorted largest first.
pub fn filter_by_min_size(arts: &[CoverArtInfo], min_size: CoverArtSize) -> Vec<&CoverArtInfo> {
    let mut filtered: Vec<_> = arts.iter().filter(|a| classify(a) >= min_size).collect();
    filtered.sort_by(|a, b| {
        let a_dim = a.width.unwrap_or(0).max(a.height.unwrap_or(0));
        let b_dim = b.width.unwrap_or(0).max(b.height.unwrap_or(0));
        b_dim.cmp(&a_dim)
    });
    filtered
}

/// Check if a URL looks like a valid cover art URL.
pub fn is_valid_art_url(url: &str) -> bool {
    (url.starts_with("http://") || url.starts_with("https://")) && url.len() > 10
}

/// Check if a URL has a recognized image file extension.
pub fn url_has_image_extension(url: &str) -> bool {
    let lower = url.to_lowercase();
    // Strip query parameters
    let path = lower.split('?').next().unwrap_or(&lower);
    path.ends_with(".jpg")
        || path.ends_with(".jpeg")
        || path.ends_with(".png")
        || path.ends_with(".webp")
}

/// Infer a MIME type from a URL's extension. Defaults to `image/jpeg`.
pub fn mime_type_for_url(url: &str) -> &'static str {
    let lower = url.to_lowercase();
    let path = lower.split('?').next().unwrap_or(&lower);
    if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".webp") {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

/// Deduplicate cover art by URL, preserving priority order.
pub fn deduplicate(arts: &[CoverArtInfo]) -> Vec<CoverArtInfo> {
    let mut seen = std::collections::HashSet::new();
    arts.iter()
        .filter(|a| seen.insert(a.url.clone()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(url: &str, w: u32, h: u32) -> CoverArtInfo {
        CoverArtInfo {
            url: url.to_string(),
            width: Some(w),
            height: Some(h),
            mime_type: None,
        }
    }

    #[test]
    fn size_classification() {
        assert_eq!(CoverArtSize::from_dimension(0), CoverArtSize::Unknown);
        assert_eq!(CoverArtSize::from_dimension(100), CoverArtSize::Thumbnail);
        assert_eq!(CoverArtSize::from_dimension(300), CoverArtSize::Small);
        assert_eq!(CoverArtSize::from_dimension(600), CoverArtSize::Medium);
        assert_eq!(CoverArtSize::from_dimension(1500), CoverArtSize::Large);
        assert_eq!(CoverArtSize::from_dimension(3000), CoverArtSize::ExtraLarge);
    }

    #[test]
    fn select_largest_works() {
        let arts = vec![art("a", 100, 100), art("b", 500, 500), art("c", 300, 300)];
        assert_eq!(select_largest(&arts).unwrap().url, "b");
    }

    #[test]
    fn select_smallest_works() {
        let arts = vec![art("a", 500, 500), art("b", 100, 100), art("c", 300, 300)];
        assert_eq!(select_smallest(&arts).unwrap().url, "b");
    }

    #[test]
    fn select_best_with_min_size() {
        let arts = vec![art("a", 100, 100), art("b", 600, 600), art("c", 1500, 1500)];
        let best = select_best(&arts, CoverArtSize::Medium).unwrap();
        assert_eq!(best.url, "b"); // Smallest that meets minimum
    }

    #[test]
    fn url_validation() {
        assert!(is_valid_art_url("https://example.com/image.jpg"));
        assert!(is_valid_art_url("http://example.com/image.jpg"));
        assert!(!is_valid_art_url("ftp://example.com/image.jpg"));
        assert!(!is_valid_art_url("http://x"));
    }

    #[test]
    fn image_extension_detection() {
        assert!(url_has_image_extension("https://example.com/art.jpg"));
        assert!(url_has_image_extension("https://example.com/art.PNG"));
        assert!(url_has_image_extension("https://example.com/art.webp?size=500"));
        assert!(!url_has_image_extension("https://example.com/art.gif"));
    }

    #[test]
    fn mime_type_inference() {
        assert_eq!(mime_type_for_url("https://example.com/art.png"), "image/png");
        assert_eq!(mime_type_for_url("https://example.com/art.webp"), "image/webp");
        assert_eq!(mime_type_for_url("https://example.com/art.jpg"), "image/jpeg");
        assert_eq!(mime_type_for_url("https://example.com/art"), "image/jpeg");
    }

    #[test]
    fn deduplication() {
        let arts = vec![
            art("a", 100, 100),
            art("b", 200, 200),
            art("a", 300, 300), // Duplicate URL
        ];
        let deduped = deduplicate(&arts);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].url, "a");
        assert_eq!(deduped[0].width, Some(100)); // First occurrence kept
    }
}
