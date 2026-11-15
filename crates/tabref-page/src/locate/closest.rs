//! The names nearest a target that matched nothing, for the error that
//! says what to try instead.

use super::name_match::normalize;

const MIN_SCORE: f64 = 0.25;

/// How alike two normalized names are, from 0 to 1: one inside the other
/// scores by how much of the longer it covers, otherwise by the share of
/// character pairs they have in common.
pub(crate) fn similarity(query: &str, name: &str) -> f64 {
    let (query, name) = (normalize(query), normalize(name));
    if query.is_empty() || name.is_empty() {
        return 0.0;
    }
    let (short, long) = if query.len() <= name.len() {
        (&query, &name)
    } else {
        (&name, &query)
    };
    if long.contains(short.as_str()) {
        return 0.6 + 0.4 * short.chars().count() as f64 / long.chars().count() as f64;
    }
    let pairs = |text: &str| {
        let chars: Vec<char> = text.chars().collect();
        chars
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .collect::<Vec<_>>()
    };
    let (left, mut right) = (pairs(&query), pairs(&name));
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let total = left.len() + right.len();
    let mut shared = 0;
    for pair in &left {
        if let Some(position) = right.iter().position(|other| other == pair) {
            right.swap_remove(position);
            shared += 1;
        }
    }
    2.0 * f64::from(shared) / total as f64
}

/// The indices of the up to `count` names most like `query`, best first,
/// leaving out names too unlike it to help and repeats of one label.
pub(crate) fn closest(query: &str, labels: &[(usize, String)], count: usize) -> Vec<usize> {
    let mut scored: Vec<(f64, usize, &String)> = labels
        .iter()
        .map(|(index, label)| (similarity(query, label), *index, label))
        .filter(|(score, _, _)| *score >= MIN_SCORE)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut seen: Vec<&String> = Vec::new();
    let mut picked = Vec::new();
    for (_, index, label) in scored {
        if seen.contains(&label) {
            continue;
        }
        seen.push(label);
        picked.push(index);
        if picked.len() == count {
            break;
        }
    }
    picked
}

#[cfg(test)]
#[path = "closest_tests.rs"]
mod tests;
