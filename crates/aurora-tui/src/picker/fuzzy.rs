/// Score de 0 (pas de match) à 1200+ (match exact).
/// query: ce que l'utilisateur tape
/// name: nom du beam
/// description: description optionnelle
pub fn fuzzy_score(query: &str, name: &str, description: Option<&str>) -> u32 {
    if query.is_empty() {
        return 500; // tout est "matched" quand pas de query
    }
    let q = query.to_lowercase();
    let n = name.to_lowercase();

    // Match exact
    if n == q {
        return 1200;
    }
    // Sous-chaîne dans le nom
    if n.contains(&q) {
        return 800;
    }
    // Fuzzy : tous les chars de q apparaissent dans l'ordre dans n
    if chars_match_in_order(&q, &n) {
        // Score basé sur la densité (chars matchés / longueur du nom)
        let ratio = (q.len() as f32 / n.len() as f32 * 400.0) as u32;
        return 100 + ratio;
    }
    // Sous-chaîne dans la description
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

/// Retourne les indices des chars matchés dans `name` pour le highlighting.
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
