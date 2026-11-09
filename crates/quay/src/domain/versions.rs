//! Version numbers like 0.36.2, compared the way people read them.

/// `v0.36.2`, `0.36.2-1` and `0.36.2` all give [0, 36, 2]; anything unreadable gives None.
pub fn parse(text: &str) -> Option<Vec<u32>> {
    let t = text.trim();
    let t = t.strip_prefix('v').unwrap_or(t);
    let end = t.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(t.len());
    let core = t[..end].trim_end_matches('.');
    if core.is_empty() || !core.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    core.split('.').map(|p| p.parse().ok()).collect()
}

pub fn is_newer(latest: Option<&str>, installed: Option<&str>) -> bool {
    let (Some(a), Some(b)) = (latest.and_then(parse), installed.and_then(parse)) else {
        return false;
    };
    let width = a.len().max(b.len());
    let pad = |v: &[u32]| (0..width).map(|i| v.get(i).copied().unwrap_or(0)).collect::<Vec<_>>();
    pad(&a) > pad(&b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_compares_like_people() {
        assert_eq!(parse("v0.36.2"), Some(vec![0, 36, 2]));
        assert_eq!(parse("0.36.2-1"), Some(vec![0, 36, 2]));
        assert_eq!(parse("latest"), None);
        assert!(is_newer(Some("0.10.0"), Some("0.9.9")));
        assert!(!is_newer(Some("1.0"), Some("1.0.0")));
        assert!(!is_newer(None, Some("1.0")));
    }
}
