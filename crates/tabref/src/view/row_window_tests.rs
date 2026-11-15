use super::RowWindow;

#[test]
fn row_ranges_parse_open_closed_and_single() {
    let open = RowWindow::parse("21-").unwrap();
    assert!(open.contains(21) && open.contains(500) && !open.contains(20));
    let closed = RowWindow::parse("21-40").unwrap();
    assert!(closed.contains(40) && !closed.contains(41));
    let single = RowWindow::parse("5").unwrap();
    assert!(single.contains(5) && !single.contains(6));
    assert!(RowWindow::default().contains(20) && !RowWindow::default().contains(21));
}

#[test]
fn bad_ranges_are_refused() {
    for text in ["", "0-3", "9-2", "a-", "-4"] {
        assert!(RowWindow::parse(text).is_err(), "{text}");
    }
}
