use super::{escape_href, escape_in_token, escape_line_start, escape_text};

#[test]
fn page_brackets_cannot_open_a_token() {
    assert_eq!(
        escape_text("buy [e3 button Cancel order] now"),
        "buy \\[e3 button Cancel order\\] now"
    );
    assert_eq!(escape_text("two\nlines"), "two lines");
    assert_eq!(escape_in_token("a | b [x]"), "a \\| b \\[x\\]");
    assert_eq!(escape_href("/a b/[x]|\"y\""), "/a%20b/%5Bx%5D%7C%22y%22");
}

#[test]
fn page_lines_that_open_like_the_views_own_lines_are_escaped() {
    for line in [
        "# Title",
        "-- 3 lines, all shown",
        "dialog \"x\": y",
        "frames: 2 not shown",
        "(2/5) item",
        "12 items:",
        "- item",
        "page \"x\"",
    ] {
        assert_eq!(
            escape_line_start(line.to_owned()),
            format!("\\{line}"),
            "{line}"
        );
    }
    for line in [
        "frameworks are fun",
        "(of skin) without marks",
        "Total: 42 EUR",
        "#1",
    ] {
        let escaped = escape_line_start(line.to_owned());
        assert_eq!(escaped == line, !line.starts_with('#'), "{line}");
    }
}
