use tabref_cdp::PageTarget;
use tabref_core::ErrorCode;

use super::{choose_tab, find_tab};

fn tab(id: &str) -> PageTarget {
    PageTarget {
        target_id: id.into(),
        url: format!("https://{id}.test/"),
        title: id.into(),
        attached: false,
        browser_context_id: None,
    }
}

fn tabs() -> Vec<PageTarget> {
    vec![tab("AB12"), tab("CD34"), tab("CD99")]
}

#[test]
fn an_index_or_a_unique_prefix_names_a_tab() {
    assert_eq!(find_tab(&tabs(), "1").unwrap(), "CD34");
    assert_eq!(find_tab(&tabs(), "ab").unwrap(), "AB12");
    assert_eq!(find_tab(&tabs(), "CD99").unwrap(), "CD99");
}

#[test]
fn a_missing_or_shared_prefix_is_refused() {
    assert_eq!(
        find_tab(&tabs(), "7").unwrap_err().code,
        ErrorCode::WindowNotFound
    );
    assert_eq!(
        find_tab(&tabs(), "CD").unwrap_err().code,
        ErrorCode::AmbiguousTarget
    );
}

#[test]
fn the_remembered_tab_wins_while_open_and_the_first_tab_otherwise() {
    assert_eq!(
        choose_tab(&tabs(), None, Some("CD99")).unwrap().as_deref(),
        Some("CD99")
    );
    assert_eq!(
        choose_tab(&tabs(), None, Some("GONE")).unwrap().as_deref(),
        Some("AB12")
    );
    assert_eq!(
        choose_tab(&tabs(), Some("2"), Some("AB12"))
            .unwrap()
            .as_deref(),
        Some("CD99")
    );
    assert_eq!(choose_tab(&[], None, None).unwrap(), None);
}
