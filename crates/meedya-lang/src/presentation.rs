// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang::presentation — Part B of the policy: what a *person sees*
// in a menu (UI-020 to UI-070). This depends on the user's preferences
// and interface language, so it gives a different answer for different
// people — the opposite of `canonical.rs`'s Part A, which must give the
// same answer everywhere. The two are kept as genuinely separate
// algorithms here (not just separate function names wrapping the same
// comparator), because the policy explicitly forbids implementing both
// with one comparison function: preference order and "the original"
// dominate here (UI-020, UI-030) where Part A only ever cares about the
// original; and the remaining groups are ordered by a *localised name*
// (UI-040) that Part A must never look at, because a name changes with
// the interface language and stored order must not.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::canonical::{self, GroupKey, BUCKET_MALFORMED};
use crate::roles::{self, Role, RoleItem, TrackType};
use crate::tag::LanguageTag;

/// The two accessibility preferences AUTO-040 and UI-045 read: has the
/// user asked to have audio description brought forward, and has the
/// user asked to have SDH/captions brought forward. Absent (`false`)
/// means "no such preference" — this struct never distinguishes "not
/// asked" from "asked for the opposite", because there is no opposite to
/// ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Accessibility {
    pub audio_description: bool,
    pub captions: bool,
}

/// Everything [`sort_for_presentation`] and [`subtitle_menu`] need beyond
/// the items themselves: the user's language preferences, highest
/// priority first (UI-020), and their accessibility settings (AUTO-040).
/// Deliberately carries no "selected" field — UI-050 requires that
/// selecting a track never reorders the menu, and the surest way to keep
/// that true is for the ordering function to have nothing to select from
/// in the first place.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PresentationContext {
    /// Already-canonical preferences, highest priority first. A malformed
    /// preference is ignored (UI-020): it matches nothing (MATCH-010), so
    /// it brings no group forward and promotes no item.
    pub preferences: Vec<LanguageTag>,
    pub accessibility: Accessibility,
}

/// What kind of thing a presentation-list item is, for TRACK-050's
/// per-type role order and for which accessibility preference (if any)
/// can move it forward within its group (UI-045). A plain language item
/// with no track-like roles — a translation in a picker, say — reports
/// `None` from [`PresentationItem::kind`] instead of one of these.
///
/// Serialises as `audio`, `subtitle`, `text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresentationKind {
    Audio,
    Subtitle,
    Text,
}

/// Anything that can appear in a presentation list or menu: it has a
/// language and roles (via [`RoleItem`], which builds on
/// [`LanguageItem`](crate::LanguageItem); both default to "not original"
/// and "no roles"), and optionally a kind. Implement this on your own item
/// type to build a menu with [`sort_for_presentation`] or
/// [`subtitle_menu`]. A plain language item needs only an empty
/// `impl RoleItem for MyItem {}` beside this.
pub trait PresentationItem: RoleItem {
    /// `None` for a plain language item (no roles apply); `Some(kind)`
    /// for something track-shaped. Defaults to `None`.
    fn kind(&self) -> Option<PresentationKind> {
        None
    }
}

fn track_type_for_role_order(kind: Option<PresentationKind>) -> TrackType {
    match kind {
        Some(PresentationKind::Audio) => TrackType::Audio,
        Some(PresentationKind::Subtitle) => TrackType::Subtitle,
        // Text items and plain language items never rank by role
        // (`roles::role_rank` returns 0 for `TrackType::Other` regardless
        // of what roles are passed), matching how the policy's own
        // reference cases treat anything that isn't audio or subtitle.
        Some(PresentationKind::Text) | None => TrackType::Other,
    }
}

/// Builds the order groups are shown in (UI-020 to UI-040): the user's
/// preferences' groups first, in their given order; then the original
/// language's group(s), in canonical order among themselves, if not
/// already placed; then every other ordinary-language group,
/// alphabetically by localised name via `compare_groups`; then the
/// special/grandfathered/private-use groups present, in LANG-025's fixed
/// order; then the malformed group last, if any item is malformed.
///
/// Returns each group's rank (0 = shown first) so the caller can sort by
/// it directly.
fn group_order<T: PresentationItem>(
    items: &[T],
    context: &PresentationContext,
    compare_groups: &impl Fn(&str, &str) -> Ordering,
) -> HashMap<GroupKey, usize> {
    let present: HashSet<GroupKey> = items
        .iter()
        .map(|it| canonical::group_key(it.language()))
        .collect();
    let mut order: Vec<GroupKey> = Vec::new();
    let mut seen: HashSet<GroupKey> = HashSet::new();

    // UI-020: preferences, in the user's priority order. A preference's
    // group is its primary language (a preference for en-GB brings the
    // whole English group forward) — group_key already reduces to that.
    for pref in &context.preferences {
        if pref.is_malformed() {
            continue;
        }
        let g = canonical::group_key(pref);
        if present.contains(&g) && seen.insert(g.clone()) {
            order.push(g);
        }
    }

    // UI-030: the original language's group(s), in canonical order among
    // themselves, if a preference has not already placed them.
    let mut original_groups: Vec<GroupKey> = items
        .iter()
        .filter(|it| it.is_original())
        .map(|it| canonical::group_key(it.language()))
        .filter(|g| g.0 != BUCKET_MALFORMED) // malformed is never "the original" here
        .collect();
    // UI-030 (as revised): several original groups are ordered among
    // themselves the same way UI-040 orders "everything else" — ordinary
    // languages by localised name (ties by primary language code — see
    // `by_name_then_code`), so a menu with two original languages still
    // reads alphabetically rather than by code; any original special code
    // sorts after all of those, in its own fixed order.
    original_groups.sort_by(|a, b| match (a.0 == 0, b.0 == 0) {
        (true, true) => by_name_then_code(compare_groups, &a.1, &b.1),
        (false, false) => a.cmp(b),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
    });
    original_groups.dedup();
    for g in original_groups {
        if seen.insert(g.clone()) {
            order.push(g);
        }
    }

    // UI-040: every remaining ORDINARY group (bucket 0), alphabetically by
    // localised name via the caller's collation, then by primary language
    // code when two names compare equal (`by_name_then_code`).
    let mut rest: Vec<GroupKey> = present
        .iter()
        .filter(|g| g.0 == 0 && !seen.contains(*g))
        .cloned()
        .collect();
    rest.sort_by(|a, b| by_name_then_code(compare_groups, &a.1, &b.1));
    for g in rest {
        seen.insert(g.clone());
        order.push(g);
    }

    // The special codes (LANG-025) after all ordinary groups, in their
    // fixed order — `GroupKey`'s natural ordering (bucket, then the
    // group's secondary text) already matches that fixed order.
    let mut specials: Vec<GroupKey> = present
        .iter()
        .filter(|g| g.0 != 0 && g.0 != BUCKET_MALFORMED && !seen.contains(*g))
        .cloned()
        .collect();
    specials.sort();
    for g in specials {
        seen.insert(g.clone());
        order.push(g);
    }

    // Malformed values last (LANG-026), as a single group.
    let malformed_group: GroupKey = (BUCKET_MALFORMED, String::new());
    if present.contains(&malformed_group) {
        order.push(malformed_group);
    }

    order
        .into_iter()
        .enumerate()
        .map(|(rank, g)| (g, rank))
        .collect()
}

/// UI-040's group order for two ordinary groups: the caller's
/// localised-name comparison first, then — when it says the two names are
/// equal — the primary language codes as plain ASCII ("Groups with the
/// same name are ordered by primary language code").
///
/// This crate does the tie-break itself rather than trusting the caller's
/// closure to. Before policy revision 4 it did not, and a closure that
/// compared names only (a natural thing to write — a collator does exactly
/// that) left tied groups in whatever order a hash set happened to list
/// them, which changes from one run to the next.
fn by_name_then_code(
    compare_groups: &impl Fn(&str, &str) -> Ordering,
    a: &str,
    b: &str,
) -> Ordering {
    compare_groups(a, b).then_with(|| a.cmp(b))
}

/// The tuple items within one group are sorted by (UI-045):
/// `(role_rank_with_accessibility_override, exact_preference_index,
/// original_flag, specificity)`. A stable sort over this tuple keeps
/// ties in their original order (LANG-027), so there is no explicit
/// index field here either — see the note on [`canonical::compare_ranked`].
fn within_group_key<T: PresentationItem>(
    item: &T,
    context: &PresentationContext,
    preference_tags: &[String],
) -> (i16, usize, u8, canonical::SpecificityKey) {
    // LANG-026 applies inside a menu too, not only to stored order: a
    // malformed value keeps the order it was found in, and nothing about
    // it — its roles, its original flag, a preference naming it (it
    // cannot canonicalise to anything a preference could match anyway) —
    // may move it. Returning the same constant key for every malformed
    // item makes them all tie, and this function's caller sorts with a
    // *stable* sort, so a tie is resolved by original position for free.
    if item.language().is_malformed() {
        return (0, 0, 0, canonical::specificity_key(item.language()));
    }

    let kind = item.kind();
    let track_type = track_type_for_role_order(kind);
    let roles = item.roles();
    let mut role_rank = i16::from(roles::role_rank(track_type, roles));

    // AUTO-040 / UI-045 rule 1 (as revised): the promotion applies only to
    // a track whose PLACEMENT role — TRACK-050's rule, the latest of its
    // roles, which is exactly what `roles::role_rank` already computed
    // above — is the one asked for. Checking `roles.contains(...)` here
    // instead would wrongly promote a track placed by something else
    // entirely: a `["forced", "sdh"]` subtitle is placed as forced (the
    // later of the two in TRACK-050's order) and must NOT move ahead just
    // because SDH is also among its roles. -1 is below every ordinary
    // rank (0..=4), so this always wins the role-rank comparison outright
    // once it does apply.
    if context.accessibility.audio_description
        && kind == Some(PresentationKind::Audio)
        && role_rank
            == i16::from(roles::individual_rank(
                TrackType::Audio,
                Role::AudioDescription,
            ))
    {
        role_rank = -1;
    }
    if context.accessibility.captions
        && kind == Some(PresentationKind::Subtitle)
        && role_rank == i16::from(roles::individual_rank(TrackType::Subtitle, Role::Sdh))
    {
        role_rank = -1;
    }

    // UI-045 rule 2: only an EXACT preference promotes within a group —
    // en is not promoted by a preference for en-GB, only en-GB itself is.
    let preference_index = preference_tags
        .iter()
        .position(|p| p == &item.language().tag)
        .unwrap_or(preference_tags.len());

    let original_rank = u8::from(!item.is_original());

    (
        role_rank,
        preference_index,
        original_rank,
        canonical::specificity_key(item.language()),
    )
}

/// Sorts `items` into Part B's presentation order (UI-020 to UI-050):
/// the user's preferred language groups first, in their order; then the
/// original language's group, if not already placed; then everything
/// else, alphabetically by localised name (via `compare_groups`, since
/// this crate holds no name data of its own); special codes, then
/// malformed values, last. Within a group, an accessibility role the
/// user asked for leads, then an exact preference match, then the
/// original item, then specificity, then stability.
///
/// `compare_groups` compares two primary-language subtags using the
/// caller's localised-name collation for the current interface language
/// — real callers use platform locale data (`Intl.DisplayNames`,
/// `NSLocale`, ICU); this crate never invents its own name list. It only
/// has to compare the NAMES: when it returns [`Ordering::Equal`] this
/// function breaks the tie by primary language code itself (UI-040).
///
/// This function never takes a "selected" item (UI-050): selecting a
/// track is shown with a tick or highlight, never by moving it — an
/// ordering function with no such parameter cannot be tempted to do that.
pub fn sort_for_presentation<T: PresentationItem>(
    items: &mut [T],
    context: &PresentationContext,
    compare_groups: impl Fn(&str, &str) -> Ordering,
) {
    let order = group_order(items, context, &compare_groups);
    let preference_tags: Vec<String> = context
        .preferences
        .iter()
        .filter(|p| !p.is_malformed())
        .map(|p| p.tag.clone())
        .collect();

    items.sort_by(|a, b| {
        let a_rank = order[&canonical::group_key(a.language())];
        let b_rank = order[&canonical::group_key(b.language())];
        a_rank.cmp(&b_rank).then_with(|| {
            within_group_key(a, context, &preference_tags).cmp(&within_group_key(
                b,
                context,
                &preference_tags,
            ))
        })
    });
}

/// One row of a subtitle menu (UI-060): the fixed "Off" entry, which is
/// not a language and is never sorted with the tracks, or a reference to
/// one of the (already sorted) subtitle tracks.
///
/// Always `Clone` and `Copy` (it holds only a reference), whatever `T`
/// is. Two entries are equal when both are `Off`, or both are tracks
/// that compare equal (`T: PartialEq`).
#[derive(Debug)]
pub enum MenuEntry<'a, T> {
    Off,
    Track(&'a T),
}

// Written by hand, not derived: `#[derive(Clone, Copy)]` would demand
// `T: Clone`/`T: Copy`, although copying a reference never needs either.
impl<T> Clone for MenuEntry<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for MenuEntry<'_, T> {}

impl<T: PartialEq> PartialEq for MenuEntry<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MenuEntry::Off, MenuEntry::Off) => true,
            (MenuEntry::Track(a), MenuEntry::Track(b)) => a == b,
            _ => false,
        }
    }
}

impl<T: Eq> Eq for MenuEntry<'_, T> {}

/// Builds a subtitle menu per UI-060: sorts `items` into presentation
/// order (exactly as [`sort_for_presentation`] does — this is not a
/// second algorithm) and returns them with a fixed [`MenuEntry::Off`]
/// first. Callers pass only the subtitle tracks in `items`; this
/// function does not filter by track type itself.
pub fn subtitle_menu<'a, T: PresentationItem>(
    items: &'a mut [T],
    context: &PresentationContext,
    compare_groups: impl Fn(&str, &str) -> Ordering,
) -> Vec<MenuEntry<'a, T>> {
    sort_for_presentation(items, context, compare_groups);
    let mut menu = Vec::with_capacity(items.len() + 1);
    menu.push(MenuEntry::Off);
    menu.extend(items.iter().map(MenuEntry::Track));
    menu
}

/// Builds a menu label per UI-070: the language name, then each present
/// role's localised name — once each, ordered by TRACK-050 for the given
/// track type, via `role_name` — then the channel layout if given, all
/// joined with " — " (space, em dash U+2014, space).
///
/// A role listed twice appears once ("Roles appear once each"). Roles
/// TRACK-050 ranks the same (for audio, `sdh`, `forced` and `other` are all
/// "anything else") keep the order they were given in. (Before policy
/// revision 4 a repeated role was shown twice.)
///
/// `language_name` is a localised name resolved by the caller (this
/// crate holds no name data); an embedded track title is never used here
/// in its place — UI-070 forbids that when language data exists.
///
/// An empty part adds nothing — not even a separator: an empty
/// `language_name`, a role whose `role_name` is empty, or `Some("")` for
/// `channels` is left out, so `channels: Some("")` gives `English`, not
/// `English — `. (Until Codex's review r7 an empty part was joined like any
/// other, which disagreed with the PHP implementation; the case file's
/// `label-06` and `label-07` now pin it.)
pub fn label(
    track_type: TrackType,
    language_name: &str,
    roles: &[Role],
    role_name: impl Fn(Role) -> String,
    channels: Option<&str>,
) -> String {
    // Duplicates are removed BEFORE sorting, keeping the first of each:
    // removing neighbours after a sort would miss a repeat separated by a
    // different role of the same rank (`[sdh, other, sdh]` on audio).
    let mut ordered_roles: Vec<Role> = Vec::with_capacity(roles.len());
    for &role in roles {
        if !ordered_roles.contains(&role) {
            ordered_roles.push(role);
        }
    }
    ordered_roles.sort_by_key(|&r| roles::individual_rank(track_type, r));

    // Only non-empty parts are joined (see the doc comment): an empty one
    // would otherwise leave a separator with nothing after it.
    let mut parts: Vec<String> = Vec::with_capacity(2 + ordered_roles.len());
    if !language_name.is_empty() {
        parts.push(language_name.to_string());
    }
    for role in ordered_roles {
        let name = role_name(role);
        if !name.is_empty() {
            parts.push(name);
        }
    }
    if let Some(ch) = channels.filter(|ch| !ch.is_empty()) {
        parts.push(ch.to_string());
    }
    parts.join(" — ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::LanguageItem;
    use crate::tag::canonicalise;

    struct Item {
        id: &'static str,
        tag: LanguageTag,
        kind: Option<PresentationKind>,
        roles: Vec<Role>,
        original: bool,
    }

    impl LanguageItem for Item {
        fn language(&self) -> &LanguageTag {
            &self.tag
        }
        fn is_original(&self) -> bool {
            self.original
        }
    }

    impl RoleItem for Item {
        fn roles(&self) -> &[Role] {
            &self.roles
        }
    }

    impl PresentationItem for Item {
        fn kind(&self) -> Option<PresentationKind> {
            self.kind
        }
    }

    fn plain(id: &'static str, tag: &str, original: bool) -> Item {
        Item {
            id,
            tag: canonicalise(tag),
            kind: None,
            roles: Vec::new(),
            original,
        }
    }

    fn prefs(tags: &[&str]) -> Vec<LanguageTag> {
        tags.iter().map(|t| canonicalise(t)).collect()
    }

    /// Stand-in collation for tests: alphabetise by an English name table,
    /// then by primary language code on a tie — exactly the shape the
    /// policy's own conformance fixtures use.
    fn english_names(code: &str) -> &'static str {
        match code {
            "de" => "german",
            "en" => "english",
            "es" => "spanish",
            "fr" => "french",
            "ja" => "japanese",
            "nl" => "dutch",
            other => panic!("test fixture has no English name for {other:?}"),
        }
    }

    fn compare(a: &str, b: &str) -> Ordering {
        english_names(a)
            .cmp(english_names(b))
            .then_with(|| a.cmp(b))
    }

    #[test]
    fn policy_example_japanese_original_no_preferences() {
        let mut items = vec![
            plain("en", "en", false),
            plain("ja", "ja", true),
            plain("fr", "fr", false),
            plain("de", "de", false),
            plain("es", "es", false),
            plain("nl", "nl", false),
        ];
        let context = PresentationContext::default();
        sort_for_presentation(&mut items, &context, compare);
        let order: Vec<&str> = items.iter().map(|i| i.id).collect();
        assert_eq!(order, ["ja", "nl", "en", "fr", "de", "es"]);
    }

    #[test]
    fn preferences_come_before_the_original() {
        let mut items = vec![
            plain("en", "en", false),
            plain("ja", "ja", true),
            plain("fr", "fr", false),
            plain("de", "de", false),
        ];
        let context = PresentationContext {
            preferences: prefs(&["fr", "en"]),
            accessibility: Accessibility::default(),
        };
        sort_for_presentation(&mut items, &context, compare);
        let order: Vec<&str> = items.iter().map(|i| i.id).collect();
        assert_eq!(order, ["fr", "en", "ja", "de"]);
    }

    #[test]
    fn a_selection_never_reorders_the_menu() {
        // UI-050: there is no "selected" parameter to pass in the first
        // place, so this test just re-confirms the ordinary case is
        // unaffected by whatever a caller does with its own selection
        // state elsewhere.
        let mut items = vec![
            plain("en", "en", false),
            plain("en-AU", "en-AU", false),
            plain("en-GB", "en-GB", false),
            plain("en-US", "en-US", false),
            plain("fr", "fr", false),
        ];
        let context = PresentationContext {
            preferences: prefs(&["en-GB"]),
            accessibility: Accessibility::default(),
        };
        sort_for_presentation(&mut items, &context, compare);
        let order: Vec<&str> = items.iter().map(|i| i.id).collect();
        assert_eq!(order, ["en-GB", "en", "en-AU", "en-US", "fr"]);
    }

    #[test]
    fn groups_with_equal_names_are_ordered_by_code_by_the_crate_itself() {
        // UI-040: the caller's comparison says every name is equal; the
        // crate must still give a fixed order, by primary language code.
        // Before policy revision 4 tied groups kept a hash set's order,
        // which changes between runs — with eight tied groups a run giving
        // the right order by luck is about one in forty thousand.
        let codes = ["zu", "nl", "ja", "fr", "es", "en", "de", "ar"];
        let mut items: Vec<Item> = codes.iter().map(|c| plain(c, c, false)).collect();
        let context = PresentationContext::default();
        sort_for_presentation(&mut items, &context, |_: &str, _: &str| Ordering::Equal);
        let order: Vec<&str> = items.iter().map(|i| i.id).collect();
        assert_eq!(order, ["ar", "de", "en", "es", "fr", "ja", "nl", "zu"]);
    }

    #[test]
    fn original_groups_with_equal_names_are_ordered_by_code_too() {
        let mut items = vec![
            plain("nl", "nl", true),
            plain("fr", "fr", true),
            plain("de", "de", true),
            plain("en", "en", false),
        ];
        let context = PresentationContext::default();
        sort_for_presentation(&mut items, &context, |_: &str, _: &str| Ordering::Equal);
        let order: Vec<&str> = items.iter().map(|i| i.id).collect();
        assert_eq!(order, ["de", "fr", "nl", "en"]);
    }

    #[test]
    fn a_label_lists_each_role_once() {
        let name = |r: Role| r.as_str().to_string();
        assert_eq!(
            label(
                TrackType::Subtitle,
                "English",
                &[Role::Sdh, Role::Sdh, Role::Forced],
                name,
                None
            ),
            "English — sdh — forced"
        );
        // A repeat separated by a different role of the same rank (audio
        // ranks sdh and other alike): still shown once, first-seen order.
        assert_eq!(
            label(
                TrackType::Audio,
                "English",
                &[Role::Sdh, Role::Other, Role::Sdh],
                name,
                Some("5.1")
            ),
            "English — sdh — other — 5.1"
        );
    }

    #[test]
    fn an_empty_label_part_adds_nothing() {
        // Codex's review r7: `Some("")` channels gave "English — ", an
        // empty role name gave a doubled separator, and PHP gave neither.
        // Every empty part is left out, separator included. (The case
        // file's label-06 and label-07 check the first two in both
        // implementations; the empty language name is checked here, and
        // matches what PHP's buildLabel does.)
        let name = |r: Role| r.as_str().to_string();
        let no_name = |_: Role| String::new();
        assert_eq!(
            label(TrackType::Audio, "English", &[], name, Some("")),
            "English"
        );
        assert_eq!(
            label(
                TrackType::Audio,
                "English",
                &[Role::Commentary],
                no_name,
                Some("2.0")
            ),
            "English — 2.0"
        );
        assert_eq!(
            label(TrackType::Audio, "", &[Role::Commentary], name, Some("2.0")),
            "commentary — 2.0"
        );
        assert_eq!(label(TrackType::Subtitle, "", &[], no_name, Some("")), "");
    }

    #[test]
    fn menu_entries_copy_and_compare_without_bounds_on_copy() {
        #[derive(PartialEq)]
        struct NotCopy(u8);
        let a = NotCopy(1);
        let entry = MenuEntry::Track(&a);
        let copied = entry;
        assert!(entry == copied);
        assert!(MenuEntry::<NotCopy>::Off == MenuEntry::Off);
        assert!(entry != MenuEntry::Off);
    }
}
