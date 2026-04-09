pub fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{} B", bytes);
    } else if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    } else {
        return format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1), "1 B");
        assert_eq!(human_size(1023), "1023 B");
    }

    #[test]
    fn test_kb() {
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(2048), "2.0 KB");
        assert_eq!(human_size(1023 * 1024), "1023.0 KB");
    }

    #[test]
    fn test_mb() {
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
        assert_eq!(human_size(2 * 1024 * 1024), "2.0 MB");
        assert_eq!(human_size(1023 * 1024 * 1024), "1023.0 MB");
    }
}
