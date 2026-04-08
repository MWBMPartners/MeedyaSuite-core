use std::path::{Path, PathBuf};
use tracing::debug;

/// Resolve the path to an external tool binary.
///
/// Resolution order:
/// 1. Explicit path (if provided and exists)
/// 2. Adjacent to a reference binary (e.g., ffprobe next to ffmpeg)
/// 3. System PATH via `which`
/// 4. Platform-specific common locations
pub fn resolve_tool(
    tool_name: &str,
    explicit_path: Option<&Path>,
    adjacent_to: Option<&Path>,
) -> Option<PathBuf> {
    // 1. Explicit path
    if let Some(path) = explicit_path {
        if path.exists() {
            debug!("Found {tool_name} at explicit path: {}", path.display());
            return Some(path.to_path_buf());
        }
    }

    // 2. Adjacent to reference binary
    if let Some(ref_path) = adjacent_to {
        if let Some(parent) = ref_path.parent() {
            let adjacent = parent.join(tool_name);
            if adjacent.exists() {
                debug!("Found {tool_name} adjacent to {}: {}", ref_path.display(), adjacent.display());
                return Some(adjacent);
            }
        }
    }

    // 3. System PATH
    if let Ok(path) = which::which(tool_name) {
        debug!("Found {tool_name} on system PATH: {}", path.display());
        return Some(path);
    }

    // 4. Platform-specific common locations
    for candidate in platform_search_paths(tool_name) {
        if candidate.exists() {
            debug!("Found {tool_name} at platform path: {}", candidate.display());
            return Some(candidate);
        }
    }

    debug!("{tool_name} not found");
    None
}

/// Resolve FFprobe binary path.
pub fn resolve_ffprobe(explicit_path: Option<&Path>, ffmpeg_path: Option<&Path>) -> Option<PathBuf> {
    resolve_tool("ffprobe", explicit_path, ffmpeg_path)
}

/// Resolve MediaInfo binary path.
pub fn resolve_mediainfo(explicit_path: Option<&Path>) -> Option<PathBuf> {
    resolve_tool("mediainfo", explicit_path, None)
}

fn platform_search_paths(tool_name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from(format!("/opt/homebrew/bin/{tool_name}")));
        paths.push(PathBuf::from(format!("/usr/local/bin/{tool_name}")));
    }

    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from(format!("/usr/bin/{tool_name}")));
        paths.push(PathBuf::from(format!("/usr/local/bin/{tool_name}")));
        paths.push(PathBuf::from(format!("/snap/bin/{tool_name}")));
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            paths.push(PathBuf::from(format!("{program_files}\\{tool_name}\\{tool_name}.exe")));
        }
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_nonexistent_tool() {
        assert!(resolve_tool("nonexistent_tool_xyz_12345", None, None).is_none());
    }

    #[test]
    fn resolve_explicit_path_that_exists() {
        // /bin/sh exists on all Unix systems
        #[cfg(unix)]
        {
            let path = Path::new("/bin/sh");
            let result = resolve_tool("sh", Some(path), None);
            assert!(result.is_some());
            assert_eq!(result.unwrap(), PathBuf::from("/bin/sh"));
        }
    }

    #[test]
    fn resolve_ffprobe_shorthand() {
        // This just tests the function doesn't panic — actual availability varies
        let _ = resolve_ffprobe(None, None);
    }

    #[test]
    fn resolve_mediainfo_shorthand() {
        let _ = resolve_mediainfo(None);
    }
}
