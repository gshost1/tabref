use super::{CANONICAL_ROLES, INTERACTIVE_ROLES, is_canonical_role, is_interactive_role};

#[test]
fn role_lists_are_sorted_and_unique() {
    for list in [INTERACTIVE_ROLES, CANONICAL_ROLES] {
        let mut sorted = list.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.as_slice(), list);
    }
}

#[test]
fn every_interactive_role_is_canonical() {
    for role in INTERACTIVE_ROLES {
        assert!(is_canonical_role(role), "{role}");
        assert!(is_interactive_role(role), "{role}");
    }
}

#[test]
fn read_only_roles_are_never_interactive() {
    for role in ["statictext", "image", "group", "list", "table", "heading"] {
        assert!(!is_interactive_role(role), "{role}");
    }
}
