/// Score from 0 (no match) to 1200+ (exact match).
/// query: what the user is typing
/// name: beam name
/// description: optional description
pub fn fuzzy_score(query: &str, name: &str, description: Option<&str>) -> u32 {
    if query.is_empty() {
        return 500; // everything is "matched" when there is no query
    }
    let q = query.to_lowercase();
    let n = name.to_lowercase();

    // Exact match
    if n == q {
        return 1200;
    }
    // Substring in the name
    if n.contains(&q) {
        return 800;
    }
    // Fuzzy: all chars of q appear in order in n
    if chars_match_in_order(&q, &n) {
        // Score based on density (matched chars / name length)
        let ratio = (q.len() as f32 / n.len() as f32 * 400.0) as u32;
        return 100 + ratio;
    }
    // Substring in the description
    if let Some(desc) = description {
        if desc.to_lowercase().contains(&q) {
            return 50;
        }
    }
    0
}

fn chars_match_in_order(query: &str, target: &str) -> bool {
    let mut target_chars = target.chars();
    'outer: for qc in query.chars() {
        for tc in target_chars.by_ref() {
            if tc == qc {
                continue 'outer;
            }
        }
        return false;
    }
    true
}

/// Returns the indices of the chars matched in `name`, for highlighting.
pub fn match_indices(query: &str, name: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    let n = name.to_lowercase();
    let mut result = vec![];
    let mut name_iter = n.char_indices();
    for qc in q.chars() {
        for (i, tc) in name_iter.by_ref() {
            if tc == qc {
                result.push(i);
                break;
            }
        }
    }
    result
}
