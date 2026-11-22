use super::RowWindow;
use crate::view::view_route::ViewRoute;

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

#[test]
fn the_whole_line_command_keeps_a_chosen_window_so_line_numbers_match() {
    let route = ViewRoute::default();
    let window = |text: &str| RowWindow::parse(text).unwrap();
    assert_eq!(
        window("22-24").whole_from(&route, 85),
        "tabref snap --full --rows 22-24 --from 85"
    );
    assert_eq!(
        window("21-").whole_from(&route, 9),
        "tabref snap --full --rows 21- --from 9"
    );
    assert_eq!(
        window("5").whole_from(&route, 9),
        "tabref snap --full --rows 5 --from 9"
    );
    assert_eq!(
        RowWindow::default().whole_from(&route, 9),
        "tabref snap --full --from 9"
    );
}
