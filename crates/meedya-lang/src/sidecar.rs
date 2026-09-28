// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang::sidecar — TEXT-030: naming a sidecar file by language
// (subtitles, lyrics), and reading one back. Its own module because
// naming a file is a different job from any of tag parsing, ordering,
// matching or selection — the policy's section 9 keeps these apart.
// TEXT-030 is part of policy 1.0.0; it was tightened in the revisions made
// before that version was released, and this module follows the current
// text.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::roles::{individual_rank, Role, TrackType};
use crate::tag::from_legacy_three_letter;

/// [`build_sidecar_name`] was asked for a clash-avoiding number outside
/// the range TEXT-030 allows: 2 through 999,999,999 inclusive. `0` and
/// `1` are refused because they can never distinguish anything (there is
/// no sidecar "number 0" or "number 1" to clash with — numbering starts
/// at 2, the SECOND sidecar); a negative number makes no sense in a file
/// name at all; anything past nine digits is refused because a reader
/// treats a run of ten or more digits as plain text, not a number, so
/// building one that a reader could never parse back would silently
/// produce an unreadable name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InvalidSidecarNumber {
    pub number: i64,
}

impl fmt::Display for InvalidSidecarNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is not a valid sidecar clash-avoiding number — it must be between 2 and 999999999",
            self.number
        )
    }
}

impl std::error::Error for InvalidSidecarNumber {}

/// The parts read back out of a sidecar file name by [`parse_sidecar_name`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SidecarParts {
    /// The canonical language tag, `"und"` when the language part was
    /// unrecognised, or `None` when the file name has no language part at
    /// all (just `{stem}.{ext}`).
    pub tag: Option<String>,
    /// The language part exactly as written, when [`from_legacy_three_letter`]
    /// could not make sense of it (`tag` is then `Some("und".to_string())`
    /// alongside this) — kept so nothing is lost and a person can fix it
    /// (COMPAT-040). `None` when the language part was recognised, or
    /// when there was no language part to begin with.
    pub unrecognised: Option<String>,
    /// Roles read from role words, without repeats, in TRACK-050's
    /// subtitle order (SDH, then forced, then commentary).
    pub roles: Vec<Role>,
    /// The clash-avoiding number, if the name carried one.
    pub number: Option<u32>,
    /// Parts after the language that were neither a role word nor a
    /// number, exactly as written and in the order found — TEXT-030 says
    /// they are ignored and SHOULD be reported, so they are kept here for
    /// the caller to report (`Film.en.sdh.backup.srt` gives
    /// `["backup"]`). A run of ten or more digits is here too: it is not a
    /// number. An earlier number part overridden by a later one is not:
    /// it was a number, just not the one that counts.
    pub ignored: Vec<String>,
    /// The file extension, without its leading dot.
    pub extension: String,
}

/// The three role words a sidecar file name can carry, and the order
/// TRACK-050 puts them in for subtitles. `Role::Alternate`,
/// `Role::AudioDescription` and `Role::Other` never produce or consume a
/// word here — TEXT-030 only ever names these three.
const SIDECAR_ROLES: [Role; 3] = [Role::Sdh, Role::Forced, Role::Commentary];

fn role_word(role: Role) -> &'static str {
    match role {
        Role::Sdh => "sdh",
        Role::Forced => "forced",
        Role::Commentary => "commentary",
        Role::Alternate | Role::AudioDescription | Role::Other => {
            unreachable!("only SIDECAR_ROLES are ever passed to role_word")
        }
    }
}

/// Reads a role word back into a [`Role`], case-insensitively. `cc` and
/// `hi` are accepted as `sdh` because other tools write them that way
/// (TEXT-030). Anything else is not a role word this function recognises.
fn role_from_word(word: &str) -> Option<Role> {
    match word.to_ascii_lowercase().as_str() {
        "sdh" | "cc" | "hi" => Some(Role::Sdh),
        "forced" => Some(Role::Forced),
        "commentary" => Some(Role::Commentary),
        _ => None,
    }
}

/// Builds a sidecar file name per TEXT-030:
/// `{stem}.{tag}[.{role}…][.{n}].{extension}`.
///
/// `tag` is a raw language value, read with LANG-002's reader
/// ([`from_legacy_three_letter`]) exactly as [`parse_sidecar_name`] will
/// read the name back — so `fre` is written as `fr`, and a value the
/// reader does not recognise (a malformed value, or an unregistered
/// three-letter code such as `zzz`) as `und`. What this writes is
/// therefore always what the reader reads back. A malformed value never
/// reaches a file name — it can hold characters unsafe in a path. A
/// grandfathered or private-use tag keeps its own canonical text.
///
/// (Before policy revision 4 the builder canonicalised the value as a
/// tag instead, which wrote `fre` and `zzz` into names unchanged — names a
/// reader then read back as `fr` and `und`.)
///
/// Only `sdh`, `forced` and `commentary` ever produce a role word, in
/// that TRACK-050 order, without repeats, regardless of `roles`' own
/// order or duplicates — any other role in `roles` is silently dropped,
/// since this format has no word for it.
///
/// `number` is written as its own dot-separated part (`.2`, `.3`, …) only
/// when given — used to tell two sidecars with everything else the same
/// apart. Refuses (`Err`) a number that is not `None` and not between 2
/// and 999,999,999 inclusive — see [`InvalidSidecarNumber`].
///
/// `number` takes a signed `i64`, not the unsigned type the value ends up
/// stored as, specifically so a caller's negative number is something
/// this function can receive and refuse — an unsigned parameter type
/// would make "refuse a negative number" a claim this function could
/// never actually be asked to keep, since a negative value could never
/// reach it in the first place.
pub fn build_sidecar_name(
    stem: &str,
    tag: &str,
    roles: &[Role],
    extension: &str,
    number: Option<i64>,
) -> Result<String, InvalidSidecarNumber> {
    if let Some(n) = number {
        if !(2..=999_999_999).contains(&n) {
            return Err(InvalidSidecarNumber { number: n });
        }
    }

    let tag_text = match from_legacy_three_letter(tag) {
        Some(read) => read.tag,
        None => "und".to_string(),
    };

    let mut role_words: Vec<&'static str> = Vec::new();
    for &candidate in &SIDECAR_ROLES {
        if roles.contains(&candidate) {
            role_words.push(role_word(candidate));
        }
    }

    let mut parts: Vec<String> = Vec::with_capacity(4 + role_words.len());
    parts.push(stem.to_string());
    parts.push(tag_text);
    parts.extend(role_words.into_iter().map(String::from));
    if let Some(n) = number {
        parts.push(n.to_string());
    }
    parts.push(extension.to_string());
    Ok(parts.join("."))
}

/// Reads a sidecar file name back into its parts, per TEXT-030.
///
/// Returns `None` when `filename` does not start with `stem` followed by
/// a dot — it does not belong to that media file at all, so there is
/// nothing to parse. Otherwise the first dot-separated part after the
/// stem is always read as the language (LANG-002's legacy reader, so
/// `eng` and other old three-letter forms are understood, not just
/// canonical tags) — never a role word, even if it happens to look like
/// one, which is why a role word is never mistaken for a language
/// (`sdh` is also the code for Southern Kurdish, and `hi` for Hindi: only
/// the FIRST part is ever read as a language, so position — not spelling
/// — decides). Remaining parts are read as a role word, a number, or
/// ignored if neither — and an ignored part is returned in
/// [`SidecarParts::ignored`] so the caller can report it.
pub fn parse_sidecar_name(stem: &str, filename: &str) -> Option<SidecarParts> {
    let prefix = format!("{stem}.");
    let rest = filename.strip_prefix(&prefix)?;

    let mut segments: Vec<&str> = rest.split('.').collect();
    // `rest` is never empty here: `filename` must have at least one `.`
    // after the prefix for `strip_prefix` to have matched anything
    // meaningful, and `split('.')` on a non-empty string always yields at
    // least one segment.
    let extension = segments.pop().unwrap_or_default().to_string();

    if segments.is_empty() {
        return Some(SidecarParts {
            tag: None,
            unrecognised: None,
            roles: Vec::new(),
            number: None,
            ignored: Vec::new(),
            extension,
        });
    }

    let language_part = segments.remove(0);
    let (tag, unrecognised) = match from_legacy_three_letter(language_part) {
        Some(t) => (Some(t.tag), None),
        None => (Some("und".to_string()), Some(language_part.to_string())),
    };

    let mut roles: Vec<Role> = Vec::new();
    let mut number: Option<u32> = None;
    let mut ignored: Vec<String> = Vec::new();
    for segment in segments {
        // A number part is one to nine ASCII digits (TEXT-030, as
        // revised) — nine digits is the most that fits in the builder's
        // own 999,999,999 ceiling. A LONGER run of digits is not a
        // number at all; it falls through and is ignored below, exactly
        // like any other part this format has no meaning for. If more
        // than one part looks like a number, the LAST one counts —
        // simply overwriting `number` on every match already gives that.
        let is_number_shaped = !segment.is_empty()
            && segment.len() <= 9
            && segment.bytes().all(|b| b.is_ascii_digit());
        if is_number_shaped {
            if let Ok(n) = segment.parse::<u32>() {
                number = Some(n);
            }
            continue;
        }
        if let Some(role) = role_from_word(segment) {
            if !roles.contains(&role) {
                roles.push(role);
            }
            continue;
        }
        // Anything else is ignored, per TEXT-030 — and kept so the caller
        // can report it (a SHOULD in TEXT-030).
        ignored.push(segment.to_string());
    }
    roles.sort_by_key(|&r| individual_rank(TrackType::Subtitle, r));

    Some(SidecarParts {
        tag,
        unrecognised,
        roles,
        number,
        ignored,
        extension,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_plain_name() {
        assert_eq!(
            build_sidecar_name("Film", "en-GB", &[], "srt", None),
            Ok("Film.en-GB.srt".to_string())
        );
    }

    #[test]
    fn role_words_are_reordered_to_track_050_and_deduplicated() {
        assert_eq!(
            build_sidecar_name(
                "Film",
                "EN",
                &[Role::Forced, Role::Sdh, Role::Forced],
                "srt",
                None
            ),
            Ok("Film.en.sdh.forced.srt".to_string())
        );
    }

    #[test]
    fn a_malformed_tag_becomes_und_not_unsafe_text() {
        assert_eq!(
            build_sidecar_name("Film", "English", &[], "srt", None),
            Ok("Film.und.srt".to_string())
        );
    }

    #[test]
    fn a_trailing_newline_inside_a_subtag_still_makes_it_malformed() {
        // canon-49: any check for "is this a valid subtag" must anchor at
        // the true end of the text, not just before a trailing newline —
        // a regex bug PHP and the throwaway Python reference both had.
        // This crate never used a regex for that check, so it was never
        // exposed to the bug, but the case is worth pinning down anyway.
        assert_eq!(
            build_sidecar_name("Film", "en-x-foo\n-bar", &[], "srt", None),
            Ok("Film.und.srt".to_string())
        );
    }

    #[test]
    fn roles_with_no_sidecar_word_are_dropped() {
        assert_eq!(
            build_sidecar_name("Film", "de", &[Role::Other], "srt", None),
            Ok("Film.de.srt".to_string())
        );
    }

    #[test]
    fn a_number_is_included_only_when_given() {
        assert_eq!(
            build_sidecar_name("Mr. Robot", "fr-CA", &[Role::Commentary], "srt", Some(2)),
            Ok("Mr. Robot.fr-CA.commentary.2.srt".to_string())
        );
    }

    #[test]
    fn the_largest_valid_number_is_accepted() {
        assert_eq!(
            build_sidecar_name("Film", "en", &[], "srt", Some(999_999_999)),
            Ok("Film.en.999999999.srt".to_string())
        );
    }

    #[test]
    fn zero_and_one_are_refused() {
        assert_eq!(
            build_sidecar_name("Film", "en", &[], "srt", Some(0)),
            Err(InvalidSidecarNumber { number: 0 })
        );
        assert_eq!(
            build_sidecar_name("Film", "en", &[], "srt", Some(1)),
            Err(InvalidSidecarNumber { number: 1 })
        );
    }

    #[test]
    fn a_negative_number_is_refused() {
        assert_eq!(
            build_sidecar_name("Film", "en", &[], "srt", Some(-1)),
            Err(InvalidSidecarNumber { number: -1 })
        );
    }

    #[test]
    fn a_number_past_the_ceiling_is_refused() {
        assert_eq!(
            build_sidecar_name("Film", "en", &[], "srt", Some(1_000_000_000)),
            Err(InvalidSidecarNumber {
                number: 1_000_000_000
            })
        );
    }

    #[test]
    fn parses_a_plain_name() {
        let parts = parse_sidecar_name("Mr. Robot", "Mr. Robot.en.sdh.srt").unwrap();
        assert_eq!(parts.tag.as_deref(), Some("en"));
        assert_eq!(parts.unrecognised, None);
        assert_eq!(parts.roles, vec![Role::Sdh]);
        assert_eq!(parts.number, None);
        assert_eq!(parts.extension, "srt");
    }

    #[test]
    fn a_language_shaped_role_word_is_read_as_a_language_when_first() {
        // "sdh" is also the ISO 639-3 code for Southern Kurdish; because it
        // is the FIRST part, it is read as the language, not a role.
        let parts = parse_sidecar_name("Film", "Film.sdh.srt").unwrap();
        assert_eq!(parts.tag.as_deref(), Some("sdh"));
        assert!(parts.roles.is_empty());
    }

    #[test]
    fn cc_and_hi_are_read_as_sdh() {
        let parts = parse_sidecar_name("Film", "Film.en.hi.srt").unwrap();
        assert_eq!(parts.roles, vec![Role::Sdh]);
        let parts = parse_sidecar_name("Film", "Film.en.cc.2.srt").unwrap();
        assert_eq!(parts.roles, vec![Role::Sdh]);
        assert_eq!(parts.number, Some(2));
    }

    #[test]
    fn no_language_part_at_all() {
        let parts = parse_sidecar_name("Film", "Film.srt").unwrap();
        assert_eq!(parts.tag, None);
        assert_eq!(parts.unrecognised, None);
        assert!(parts.roles.is_empty());
        assert_eq!(parts.extension, "srt");
    }

    #[test]
    fn a_file_that_does_not_belong_to_the_stem_is_not_parsed() {
        assert_eq!(parse_sidecar_name("Film", "Other.en.srt"), None);
    }

    #[test]
    fn an_unrecognised_language_part_is_kept_alongside_und() {
        let parts = parse_sidecar_name("Film", "Film.english.srt").unwrap();
        assert_eq!(parts.tag.as_deref(), Some("und"));
        assert_eq!(parts.unrecognised.as_deref(), Some("english"));
    }

    #[test]
    fn multiple_role_words_come_back_deduplicated_and_ordered() {
        let parts = parse_sidecar_name("Film", "Film.en.sdh.cc.forced.vtt").unwrap();
        assert_eq!(parts.roles, vec![Role::Sdh, Role::Forced]);
    }

    #[test]
    fn a_run_of_ten_digits_is_not_a_number() {
        let parts = parse_sidecar_name("Film", "Film.en.1234567890.srt").unwrap();
        assert_eq!(parts.number, None);
    }

    #[test]
    fn nine_digits_is_the_largest_valid_number() {
        let parts = parse_sidecar_name("Film", "Film.en.999999999.srt").unwrap();
        assert_eq!(parts.number, Some(999_999_999));
    }

    #[test]
    fn several_number_parts_the_last_one_counts() {
        let parts = parse_sidecar_name("Film", "Film.en.2.3.srt").unwrap();
        assert_eq!(parts.number, Some(3));
    }

    #[test]
    fn an_unrecognised_part_is_ignored_alongside_a_real_role() {
        let parts = parse_sidecar_name("Film", "Film.en.sdh.backup.srt").unwrap();
        assert_eq!(parts.roles, vec![Role::Sdh]);
        assert_eq!(parts.number, None);
        assert_eq!(parts.ignored, vec!["backup".to_string()]);
    }

    #[test]
    fn ignored_parts_are_reported_in_order_and_numbers_are_not() {
        let parts = parse_sidecar_name("Film", "Film.en.x.2.1234567890.CC.old.3.srt").unwrap();
        assert_eq!(parts.roles, vec![Role::Sdh]);
        assert_eq!(parts.number, Some(3));
        assert_eq!(
            parts.ignored,
            vec!["x".to_string(), "1234567890".to_string(), "old".to_string()]
        );
    }

    #[test]
    fn the_builder_reads_its_input_like_a_reader() {
        // TEXT-030 (policy revision 4): an old three-letter code is
        // written as the tag a reader reads back; an unrecognised one as
        // und. Both used to be written unchanged.
        assert_eq!(
            build_sidecar_name("Film", "fre", &[], "srt", None),
            Ok("Film.fr.srt".to_string())
        );
        assert_eq!(
            build_sidecar_name("Film", "zzz", &[], "srt", None),
            Ok("Film.und.srt".to_string())
        );
        assert_eq!(
            build_sidecar_name("Film", "fre-ca", &[Role::Sdh], "srt", None),
            Ok("Film.fr-CA.sdh.srt".to_string())
        );
        // What the builder writes, the reader reads back.
        for value in [
            "fre",
            "zzz",
            "eng",
            "x-foo",
            "i-default",
            "XXX",
            "en-GB",
            "English",
        ] {
            let name = build_sidecar_name("Film", value, &[], "srt", None).unwrap();
            let read = parse_sidecar_name("Film", &name).unwrap();
            assert_eq!(read.unrecognised, None, "{value} -> {name}");
            let written = name
                .strip_prefix("Film.")
                .unwrap()
                .strip_suffix(".srt")
                .unwrap();
            assert_eq!(read.tag.as_deref(), Some(written), "{value} -> {name}");
        }
    }
}
