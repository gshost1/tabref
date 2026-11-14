use super::view_line::ViewLine;

/// The lines of the section under the first heading whose text contains
/// `wanted`, ignoring case: that heading and every line up to the next
/// heading of the same or a higher level. `None` when no heading matches.
pub(crate) fn section_lines(lines: Vec<ViewLine>, wanted: &str) -> Option<Vec<ViewLine>> {
    let needle = wanted.trim().to_lowercase();
    let start = lines.iter().position(|line| {
        line.heading.is_some() && line.text.trim_start_matches('#').to_lowercase().contains(&needle)
    })?;
    let level = lines[start].heading;
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.heading.is_some_and(|next| Some(next) <= level))
        .map_or(lines.len(), |offset| start + 1 + offset);
    let base = lines[start].depth;
    Some(
        lines
            .into_iter()
            .take(end)
            .skip(start)
            .map(|mut line| {
                line.depth = line.depth.saturating_sub(base);
                line.chrome = None;
                line
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "section_tests.rs"]
mod tests;
