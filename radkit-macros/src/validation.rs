/// Common MIME types that are valid for skills
pub const VALID_MIME_TYPES: &[&str] = &[
    // Wildcards
    "*/*",
    "text/*",
    "application/*",
    "image/*",
    "audio/*",
    "video/*",
    // Text types
    "text/plain",
    "text/html",
    "text/markdown",
    "text/csv",
    "text/xml",
    "text/css",
    "text/javascript",
    // Application types
    "application/json",
    "application/xml",
    "application/yaml",
    "application/x-yaml",
    "application/pdf",
    "application/zip",
    "application/octet-stream",
    "application/x-www-form-urlencoded",
    "application/javascript",
    // Image types
    "image/png",
    "image/jpeg",
    "image/jpg",
    "image/gif",
    "image/svg+xml",
    "image/webp",
    "image/bmp",
    // Audio types
    "audio/mpeg",
    "audio/wav",
    "audio/ogg",
    "audio/webm",
    // Video types
    "video/mp4",
    "video/mpeg",
    "video/webm",
    "video/ogg",
];

/// Validates a MIME type string
pub fn is_valid_mime_type(mime_type: &str) -> bool {
    VALID_MIME_TYPES.contains(&mime_type)
}

/// Suggests similar MIME types for a given invalid type
pub fn suggest_mime_type(invalid: &str) -> Vec<&'static str> {
    let invalid_lower = invalid.to_lowercase();

    VALID_MIME_TYPES
        .iter()
        .filter(|valid| {
            // Check if the valid type contains the invalid string or vice versa
            let valid_lower = valid.to_lowercase();
            valid_lower.contains(&invalid_lower) || invalid_lower.contains(&valid_lower)
        })
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_mime_types() {
        assert!(is_valid_mime_type("text/plain"));
        assert!(is_valid_mime_type("application/json"));
        assert!(is_valid_mime_type("image/png"));
        assert!(is_valid_mime_type("*/*"));
    }

    #[test]
    fn test_invalid_mime_types() {
        assert!(!is_valid_mime_type("text/invalid"));
        assert!(!is_valid_mime_type("invalid/type"));
    }

    #[test]
    fn test_suggestions() {
        let suggestions = suggest_mime_type("json");
        assert!(suggestions.contains(&"application/json"));

        let suggestions = suggest_mime_type("text/plai");
        assert!(suggestions.contains(&"text/plain"));
    }
}
