use super::ref_able::is_ref_able;
use super::ref_entry::RefEntry;
use super::ref_id::RefId;
use super::ref_map::RefMap;
use crate::AccessibilityNode;

/// Gives every ref-able node that carries an element token a ref qualified
/// by `snapshot_id`, in depth-first pre-order, and records it in `map`. A
/// ref-able node without a token cannot be addressed again and gets none.
/// Returns how many refs were allocated.
pub fn allocate_refs(tree: &mut AccessibilityNode, snapshot_id: &str, map: &mut RefMap) -> usize {
    let before = map.len();
    let mut pending: Vec<&mut AccessibilityNode> = vec![tree];
    while let Some(node) = pending.pop() {
        if is_ref_able(node)
            && let Some(token) = node.identity.retained_object.clone()
        {
            let element = map.push(RefEntry {
                role: node.role.clone(),
                name: node.identity_name().map(str::to_owned),
                token,
            });
            node.ref_id = Some(RefId::new(snapshot_id, element).to_string());
        }
        pending.extend(node.children.iter_mut().rev());
    }
    map.len() - before
}

#[cfg(test)]
#[path = "ref_alloc_tests.rs"]
mod tests;
