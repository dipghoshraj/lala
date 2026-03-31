/// Split `text` into overlapping character windows.
///
/// Edge cases:
///   - Empty text → empty Vec
///   - Text shorter than `chunk_size` → single-element Vec containing the full text
///   - `overlap >= chunk_size` → treated as `overlap = 0` (no overlap)
pub fn chunk(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    if text.is_empty() || chunk_size == 0 {
        return Vec::new();
    }

    let overlap = if overlap >= chunk_size { 0 } else { overlap };
    let step = chunk_size - overlap;
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();

    if len <= chunk_size {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < len {
        let end = (start + chunk_size).min(len);
        let chunk_text: String = chars[start..end].iter().collect();
        chunks.push(chunk_text);

        if end == len {
            break;
        }
        start += step;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_returns_empty_vec() {
        assert!(chunk("", 512, 64).is_empty());
    }

    #[test]
    fn text_shorter_than_chunk_size_returns_single_chunk() {
        let text = "short text";
        let result = chunk(text, 512, 64);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], text);
    }

    #[test]
    fn overlap_greater_than_chunk_size_treated_as_zero() {
        let text = "a".repeat(100);
        let result = chunk(&text, 10, 20);
        assert_eq!(result.len(), 10);
    }

    #[test]
    fn chunks_overlap_correctly() {
        let text = "a".repeat(20);
        // chunk_size=10, overlap=5 → step=5
        // chunks starting at 0, 5, 10 (last chunk covers 10..20)
        let result = chunk(&text, 10, 5);
        assert_eq!(result.len(), 3);
        for c in &result {
            assert!(c.len() <= 10);
        }
    }
}
