// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// Internal helpers for reading/writing `MeedyaMeta:*` freeform atoms in
// a format-agnostic way through lofty's generic `Tag` layer.
//
// Used by every module that writes MeedyaMeta atoms (mik, ai_content,
// stems, play_history, etc.) so the convention stays consistent and the
// `ItemKey::Unknown(...)` + `insert_unchecked` dance lives in one place.

use lofty::tag::{ItemKey, ItemValue, Tag, TagItem};

/// Namespace string used for all MeedyaSuite-internal atoms.
pub(crate) const MEEDYA_NAMESPACE: &str = "MeedyaMeta";

/// Compose the `----:MeedyaMeta:<name>` identifier string used as the
/// lofty `ItemKey::Unknown` payload for MeedyaMeta atoms.
pub(crate) fn meedya_item_key(name: &str) -> ItemKey {
    ItemKey::Unknown(format!("----:{MEEDYA_NAMESPACE}:{name}"))
}

/// Write a string value to a `MeedyaMeta:<name>` atom.
///
/// Uses `insert_unchecked` because lofty's normal `insert_text` rejects
/// `ItemKey::Unknown` for tag types that don't recognise the unknown key;
/// MeedyaMeta atoms are intentionally non-standard and the abstract tag
/// shouldn't gatekeep them.
pub(crate) fn write_meedya_atom(tag: &mut Tag, name: &str, value: &str) {
    tag.insert_unchecked(TagItem::new(
        meedya_item_key(name),
        ItemValue::Text(value.to_owned()),
    ));
}

/// Read a string value from a `MeedyaMeta:<name>` atom, if present.
pub(crate) fn read_meedya_atom<'a>(tag: &'a Tag, name: &str) -> Option<&'a str> {
    tag.get_string(&meedya_item_key(name))
}

/// Remove a `MeedyaMeta:<name>` atom from the tag, if present.
pub(crate) fn clear_meedya_atom(tag: &mut Tag, name: &str) {
    tag.remove_key(&meedya_item_key(name));
}
