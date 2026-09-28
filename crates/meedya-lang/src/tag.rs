// Copyright (c) 2026 MeedyaSuite
// Licensed under the MIT License. See LICENSE file in the project root.
//
// meedya-lang::tag — parsing a raw language value into a canonical BCP 47
// tag, per LANG-001 of docs/standards/media-language-bcp47-policy.md.
//
// This is the one place in the crate that reads a raw string and decides
// what it means. Everything downstream (ordering, matching, selection)
// works only with the `LanguageTag` this module produces — never with raw
// strings — so a mistake here cannot be silently papered over further in
// the pipeline.

use std::collections::HashSet;

use crate::data::{self, Data};

/// What kind of BCP 47 value a [`LanguageTag`] turned out to be, per
/// LANG-001. A caller should always check this before trusting the parsed
/// fields — `language`/`script`/`region`/`variants`/`extensions` are only
/// meaningful for [`TagKind::Ordinary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagKind {
    /// A normal language tag: a primary language, optionally with script,
    /// region, variants, extensions and private-use subtags.
    Ordinary,
    /// A registry "grandfathered" tag with no single-subtag replacement
    /// (`i-default`, `cel-gaulish`) — kept exactly as the registry spells
    /// it, never reordered or split into subtags.
    Grandfathered,
    /// A tag that is nothing but private-use subtags (`x-private`).
    PrivateUse,
    /// Not a well-formed BCP 47 tag at all — a language *name*
    /// (`English`), a locale string that was never converted (`en_US`),
    /// or text with no language content at all. Kept, flagged, and (per
    /// LANG-026) always sorted after everything real.
    Malformed,
}

/// One extension subtag: a single letter other than `x`, followed by one
/// or more further subtags (`u-ca-gregory` is `{ singleton: 'u', subtags:
/// ["ca", "gregory"] }`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Extension {
    pub singleton: char,
    pub subtags: Vec<String>,
}

/// Something worth telling a person about a tag, without refusing to use
/// the tag itself (LANG-001's "kept... and SHOULD be reported", COMPAT-040's
/// "report doubt, do not resolve it by guessing"). Never affects sorting,
/// matching or selection — those all work from the canonical tag alone.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TagNote {
    /// A subtag that is well formed but is not — as far as the embedded
    /// registry snapshot knows — one the registry has ever assigned
    /// (`en-JJ`: no region `JJ` exists). Kept as written; not guessed at.
    UnregisteredSubtag { subtag: String },
    /// A subtag the registry has withdrawn with no single replacement
    /// (`CS`, Serbia and Montenegro, which split into two countries).
    /// Kept as written, because guessing which successor was meant would
    /// be inventing a fact the content never stated.
    DeprecatedNoReplacement { subtag: String },
    /// A subtag was replaced during canonicalisation, either because the
    /// registry names a Preferred-Value for it (`iw` → `he`) or because
    /// an extlang collapsed into the primary language it stood for
    /// (`zh` + `yue` → `yue`).
    SubtagReplaced { from: String, to: String },
}

/// A canonical BCP 47 language tag, and the parts it broke into on the
/// way there (LANG-001). This is the identity every other module in this
/// crate works from — ordering, matching and selection never look at a
/// raw string again once it has become a `LanguageTag`.
///
/// Equality and ordering elsewhere in the crate always go through
/// [`tag`](LanguageTag::tag) or the parsed fields, never through
/// [`notes`](LanguageTag::notes) — two tags that mean the same thing but
/// were reached by different paths (one already canonical, one via a
/// Preferred-Value replacement) end up with the same `tag` and parsed
/// fields, but not necessarily the same notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageTag {
    /// The canonical string form. For [`TagKind::Malformed`], this is the
    /// input with leading/trailing whitespace removed (LANG-001 step 1)
    /// and nothing else done to it — never a guess at what was meant.
    pub tag: String,
    pub kind: TagKind,
    /// The primary language subtag, lower case. `None` for
    /// [`TagKind::PrivateUse`], [`TagKind::Grandfathered`] and
    /// [`TagKind::Malformed`].
    pub language: Option<String>,
    /// An extlang the registry does not list, kept where it was found
    /// rather than folded into `language` (LANG-001 step 5: a
    /// *registered* extlang always replaces the language before it and
    /// never survives to here — `zh-yue-HK` becomes `yue-HK`; an
    /// unregistered one is kept — `zh-abc` stays `zh-abc`). Almost always
    /// `None`.
    pub extlang: Option<String>,
    /// The script subtag, title case (`Hant`), if the tag carries one.
    pub script: Option<String>,
    /// The region subtag: two upper-case letters, or three digits
    /// unchanged, if the tag carries one.
    pub region: Option<String>,
    /// Variant subtags, lower case, in the order they appeared.
    pub variants: Vec<String>,
    /// Extension subtags, ordered by their singleton letter (LANG-001
    /// step 6) — `a-...` before `u-...`.
    pub extensions: Vec<Extension>,
    /// Private-use subtags (the part after `x-`), lower case.
    pub private_use: Vec<String>,
    /// Things worth reporting about this tag. Never affects sorting,
    /// matching or selection — see the struct-level note above.
    pub notes: Vec<TagNote>,
}

impl LanguageTag {
    fn malformed(text: &str) -> Self {
        LanguageTag {
            tag: text.to_string(),
            kind: TagKind::Malformed,
            language: None,
            extlang: None,
            script: None,
            region: None,
            variants: Vec::new(),
            extensions: Vec::new(),
            private_use: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn grandfathered(text: String) -> Self {
        LanguageTag {
            tag: text,
            kind: TagKind::Grandfathered,
            language: None,
            extlang: None,
            script: None,
            region: None,
            variants: Vec::new(),
            extensions: Vec::new(),
            private_use: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// True for [`TagKind::Ordinary`] — the only kind whose `language`,
    /// `script`, `region`, `variants` and `extensions` fields are
    /// meaningful.
    pub fn is_ordinary(&self) -> bool {
        self.kind == TagKind::Ordinary
    }

    /// True for [`TagKind::Malformed`] (LANG-026).
    pub fn is_malformed(&self) -> bool {
        self.kind == TagKind::Malformed
    }
}

/// Parsed subtags before any registry replacement has been applied — the
/// working state `canonicalise` builds up and then rewrites in place.
/// Never exposed outside this module; [`LanguageTag`] is the public shape.
struct Parsed {
    language: Option<String>,
    extlang: Option<String>,
    script: Option<String>,
    region: Option<String>,
    variants: Vec<String>,
    extensions: Vec<Extension>,
    private_use: Vec<String>,
}

/// LANG-001 step 1's whitespace, and only this: space (U+0020), tab
/// (U+0009), line feed (U+000A) and carriage return (U+000D). Anything
/// else — a no-break space (U+00A0) included — is part of the value, and
/// a value that starts or ends with one is malformed rather than quietly
/// tidied up. Rust's `str::trim()` is deliberately NOT used here: it
/// strips every Unicode whitespace character, which is broader than what
/// this policy asks for.
fn trim_lang_whitespace(s: &str) -> &str {
    s.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r'))
}

fn is_ascii_alnum_1_8(s: &str) -> bool {
    let len = s.chars().count();
    (1..=8).contains(&len) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_ascii_alpha(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic())
}

fn is_ascii_digit_str(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn title_case_ascii(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::with_capacity(s.len());
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
            out
        }
        None => String::new(),
    }
}

/// Splits a well-formed-looking tag into subtags, per the grammar in
/// LANG-001 step 3: language, optional extlang (at most one — a second
/// three-letter subtag in that position is malformed, not a second
/// extlang), optional script, optional region, variants, extensions,
/// private use, in that order and no other. Returns `None` the moment any
/// part of the shape is wrong; every field of the shape it lists (the
/// same variant twice, the same extension letter twice, leftover subtags
/// after the grammar is satisfied) is a possible cause.
///
/// This function does not apply any registry replacement — it only
/// decides whether the shape is legal and, if so, splits it into parts,
/// already cased as LANG-001 step 7 requires (there is no reason to
/// case a subtag a second time once its final text is known).
fn parse_wellformed(s: &str, data: &Data) -> Option<Parsed> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.iter().any(|p| !is_ascii_alnum_1_8(p)) {
        return None;
    }
    let lp: Vec<String> = parts.iter().map(|p| p.to_ascii_lowercase()).collect();

    if lp[0] == "x" {
        if lp.len() < 2 {
            return None; // "x" with nothing to be private about.
        }
        return Some(Parsed {
            language: None,
            extlang: None,
            script: None,
            region: None,
            variants: Vec::new(),
            extensions: Vec::new(),
            private_use: lp[1..].to_vec(),
        });
    }

    if !(is_ascii_alpha(&lp[0]) && (2..=8).contains(&lp[0].len())) {
        return None;
    }
    // A primary language of 4-8 letters is grammar-legal but accepted
    // only if the registry actually lists it — none does today (LANG-001
    // step 3), so this is how a language *name* such as "English" or
    // "Deutsch" is refused rather than mistaken for a tag.
    if lp[0].len() >= 4 && !data.languages.contains(&lp[0]) {
        return None;
    }
    let language = lp[0].clone();
    let mut i: usize = 1;

    // At most one extlang: a three-letter subtag right after a language
    // of at most three letters. A SECOND one immediately after is
    // malformed, not "two extlangs" — this policy allows only one.
    let mut extlang = None;
    if language.len() <= 3 {
        let mut count = 0;
        while i < lp.len() && lp[i].len() == 3 && is_ascii_alpha(&lp[i]) {
            count += 1;
            if count > 1 {
                return None;
            }
            extlang = Some(lp[i].clone());
            i += 1;
        }
    }

    let mut script = None;
    if i < lp.len() && lp[i].len() == 4 && is_ascii_alpha(&lp[i]) {
        script = Some(title_case_ascii(&lp[i]));
        i += 1;
    }

    let mut region = None;
    if i < lp.len()
        && ((lp[i].len() == 2 && is_ascii_alpha(&lp[i]))
            || (lp[i].len() == 3 && is_ascii_digit_str(&lp[i])))
    {
        region = Some(lp[i].to_ascii_uppercase());
        i += 1;
    }

    let mut variants: Vec<String> = Vec::new();
    while i < lp.len()
        && ((5..=8).contains(&lp[i].len())
            || (lp[i].len() == 4 && lp[i].as_bytes()[0].is_ascii_digit()))
    {
        if variants.contains(&lp[i]) {
            return None; // the same variant twice is malformed
        }
        variants.push(lp[i].clone());
        i += 1;
    }

    let mut extensions: Vec<Extension> = Vec::new();
    let mut seen_singletons: HashSet<char> = HashSet::new();
    while i < lp.len() && lp[i].len() == 1 && lp[i] != "x" {
        let singleton = lp[i].chars().next().unwrap();
        if !seen_singletons.insert(singleton) {
            return None; // the same extension letter twice is malformed
        }
        i += 1;
        let mut subtags: Vec<String> = Vec::new();
        while i < lp.len() && (2..=8).contains(&lp[i].len()) {
            subtags.push(lp[i].clone());
            i += 1;
        }
        if subtags.is_empty() {
            return None; // a singleton with nothing after it
        }
        extensions.push(Extension { singleton, subtags });
    }

    let mut private_use: Vec<String> = Vec::new();
    if i < lp.len() && lp[i] == "x" {
        i += 1;
        while i < lp.len() {
            private_use.push(lp[i].clone());
            i += 1;
        }
        if private_use.is_empty() {
            return None; // "x" with nothing after it
        }
    }

    if i != lp.len() {
        return None; // leftover subtags the grammar above never consumed
    }

    Some(Parsed {
        language: Some(language),
        extlang,
        script,
        region,
        variants,
        extensions,
        private_use,
    })
}

/// Rebuilds the tag string from its parts, in RFC 5646 order (LANG-001
/// step 3's ordering) — used both for the final canonical text and, on
/// the not-yet-replaced parse, to look a combination up in the
/// registry's redundant-tag table.
fn join_parts(p: &Parsed) -> String {
    let mut out: Vec<String> = Vec::new();
    if let Some(l) = &p.language {
        out.push(l.clone());
    }
    if let Some(e) = &p.extlang {
        out.push(e.clone());
    }
    if let Some(s) = &p.script {
        out.push(s.clone());
    }
    if let Some(r) = &p.region {
        out.push(r.clone());
    }
    out.extend(p.variants.iter().cloned());
    for ext in &p.extensions {
        out.push(ext.singleton.to_string());
        out.extend(ext.subtags.iter().cloned());
    }
    if !p.private_use.is_empty() {
        out.push("x".to_string());
        out.extend(p.private_use.iter().cloned());
    }
    out.join("-")
}

/// Turns a raw value — as it might arrive from a file, a database, or a
/// user — into its canonical BCP 47 form, following LANG-001's seven
/// steps. Never panics: anything that is not a well-formed tag comes back
/// as [`TagKind::Malformed`] with its (trimmed) text kept, rather than
/// being dropped or guessed into something it never said.
///
/// This function does not add a script, region or variant the input did
/// not declare (LANG-024): `en` stays `en`, and `zh-TW` stays `zh-TW`,
/// however likely a longer form might seem.
pub fn canonicalise(input: &str) -> LanguageTag {
    let data = data::data();
    let trimmed = trim_lang_whitespace(input);
    if trimmed.is_empty() {
        return LanguageTag::malformed(trimmed);
    }

    // Step 2: grandfathered tags are checked, case-insensitively, before
    // the grammar — some of them (`i-default`) would not pass it anyway.
    let lower_whole = trimmed.to_ascii_lowercase();
    if let Some(entry) = data.grandfathered.get(&lower_whole) {
        return match &entry.preferred {
            Some(replacement) => canonicalise(replacement),
            None => LanguageTag::grandfathered(entry.tag.clone()),
        };
    }

    // Step 3: well-formed grammar.
    let Some(mut parsed) = parse_wellformed(trimmed, data) else {
        return LanguageTag::malformed(trimmed);
    };

    // A tag that is nothing but private-use subtags — nothing left to
    // replace or report on.
    if parsed.language.is_none() {
        let tag = join_parts(&parsed);
        return LanguageTag {
            tag,
            kind: TagKind::PrivateUse,
            language: None,
            extlang: None,
            script: None,
            region: None,
            variants: Vec::new(),
            extensions: Vec::new(),
            private_use: parsed.private_use,
            notes: Vec::new(),
        };
    }

    // Step 4: redundant tags (a specific, already-well-formed combination
    // the registry has a single-tag replacement for, such as `sgn-BR` →
    // `bzs`) — checked on the not-yet-replaced combination, because the
    // registry's redundant-tag table is keyed on exactly that combination.
    let redundant_key = join_parts(&parsed).to_ascii_lowercase();
    if let Some(replacement) = data.redundant_preferred.get(&redundant_key) {
        return canonicalise(replacement);
    }

    let mut notes: Vec<TagNote> = Vec::new();

    // Step 5 (extlang): a *registered* extlang replaces the language
    // before it — checked by membership in the registry's own table, not
    // assumed from the shape alone, because RFC 5646's grammar happily
    // parses an unregistered three-letter subtag into extlang position
    // too (`zh-abc`). An extlang the registry does not list is kept
    // exactly where it is (LANG-001 step 5's newer wording), which is why
    // `parsed.extlang` survives into the final `LanguageTag` in that case
    // — see that field's doc comment.
    if let Some(extlang) = parsed.extlang.take() {
        if let Some(replacement) = data.preferred_extlang.get(&extlang) {
            let previous_language = parsed.language.clone().unwrap();
            if replacement != &previous_language {
                notes.push(TagNote::SubtagReplaced {
                    from: previous_language,
                    to: replacement.clone(),
                });
            }
            parsed.language = Some(replacement.clone());
            // parsed.extlang stays None: the extlang has been folded away.
        } else {
            // Not registered: put it back so it survives to the final tag,
            // unchanged, in its original position (right after language),
            // and say so — this is exactly the "well formed but not in the
            // registry" situation LANG-001 asks to have reported.
            notes.push(TagNote::UnregisteredSubtag {
                subtag: extlang.clone(),
            });
            parsed.extlang = Some(extlang);
        }
    }

    // Step 5 (language, script, region, variants): each Preferred-Value
    // replacement the registry names. A subtag with no replacement is
    // checked against "deprecated with no replacement" (kept, reported)
    // and then "not in the registry at all" (kept, reported) — the two
    // are different situations and get different notes.
    let language = parsed.language.clone().unwrap();
    match data.preferred_language.get(&language) {
        Some(replacement) if replacement != &language => {
            notes.push(TagNote::SubtagReplaced {
                from: language.clone(),
                to: replacement.clone(),
            });
            parsed.language = Some(replacement.clone());
        }
        Some(_) => {}
        None => {
            if data.deprecated_language.contains(&language) {
                notes.push(TagNote::DeprecatedNoReplacement {
                    subtag: language.clone(),
                });
            } else if !data.languages.contains(&language)
                && !data::in_ranges(&language, &data.language_ranges)
            {
                notes.push(TagNote::UnregisteredSubtag {
                    subtag: language.clone(),
                });
            }
        }
    }

    if let Some(script) = parsed.script.clone() {
        match data.preferred_script.get(&script) {
            Some(replacement) if replacement != &script => {
                notes.push(TagNote::SubtagReplaced {
                    from: script.clone(),
                    to: replacement.clone(),
                });
                parsed.script = Some(replacement.clone());
            }
            Some(_) => {}
            None => {
                if data.deprecated_script.contains(&script) {
                    notes.push(TagNote::DeprecatedNoReplacement {
                        subtag: script.clone(),
                    });
                } else if !data.scripts.contains(&script)
                    && !data::in_ranges(&script, &data.script_ranges)
                {
                    notes.push(TagNote::UnregisteredSubtag { subtag: script });
                }
            }
        }
    }

    if let Some(region) = parsed.region.clone() {
        match data.preferred_region.get(&region) {
            Some(replacement) if replacement != &region => {
                notes.push(TagNote::SubtagReplaced {
                    from: region.clone(),
                    to: replacement.clone(),
                });
                parsed.region = Some(replacement.clone());
            }
            Some(_) => {}
            None => {
                if data.deprecated_region.contains(&region) {
                    notes.push(TagNote::DeprecatedNoReplacement {
                        subtag: region.clone(),
                    });
                } else if !data.regions.contains(&region)
                    && !data::in_ranges(&region, &data.region_ranges)
                {
                    notes.push(TagNote::UnregisteredSubtag { subtag: region });
                }
            }
        }
    }

    for variant in parsed.variants.iter_mut() {
        match data.preferred_variant.get(variant) {
            Some(replacement) if replacement != variant => {
                notes.push(TagNote::SubtagReplaced {
                    from: variant.clone(),
                    to: replacement.clone(),
                });
                *variant = replacement.clone();
            }
            Some(_) => {}
            None => {
                if data.deprecated_variant.contains(variant) {
                    notes.push(TagNote::DeprecatedNoReplacement {
                        subtag: variant.clone(),
                    });
                } else if !data.variants.contains(variant) {
                    notes.push(TagNote::UnregisteredSubtag {
                        subtag: variant.clone(),
                    });
                }
            }
        }
    }
    // A variant's own replacement can collide with a variant already
    // present (`ja-Latn-hepburn-heploc-alalc97`: `heploc` replaces to
    // `alalc97`, which the tag already carries) — LANG-001 is a canonical
    // FORM, so the same variant is never written twice. Keep the first
    // occurrence, drop the later one.
    let mut seen_variants: HashSet<String> = HashSet::new();
    parsed.variants.retain(|v| seen_variants.insert(v.clone()));

    // Step 6: extensions in order of their singleton letter. Parsing
    // already grouped each singleton with its own subtags; this is the
    // ordering step, done last so a replacement above can never disturb
    // it.
    parsed.extensions.sort_by_key(|e| e.singleton);

    let tag = join_parts(&parsed);

    // LANG-001 steps 4-5 repeat until the tag stops changing: a
    // replacement above (most often a region's Preferred-Value) can turn
    // the tag into something that is ITSELF a redundant or grandfathered
    // tag, even though the original input was neither (`sgn-DD` becomes
    // `sgn-DE` once the region is replaced, and `sgn-de` is redundant for
    // `gsg`). Only worth re-checking when something actually changed —
    // if the tag is unchanged, steps 4 and 2 already ran on this exact
    // text at the top of this function and found nothing.
    let tag_lower = tag.to_ascii_lowercase();
    if tag_lower != trimmed.to_ascii_lowercase()
        && (data.redundant_preferred.contains_key(&tag_lower)
            || data.grandfathered.contains_key(&tag_lower))
    {
        return canonicalise(&tag);
    }

    LanguageTag {
        tag,
        kind: TagKind::Ordinary,
        language: parsed.language,
        extlang: parsed.extlang,
        script: parsed.script,
        region: parsed.region,
        variants: parsed.variants,
        extensions: parsed.extensions,
        private_use: Vec::new(),
        notes,
    }
}

/// Splits a Matroska-style "three-letter code, hyphen, two-letter
/// country" value (`fre-ca`) into its two halves, if that is exactly the
/// shape of `s` — nothing more, nothing less (RFC 9559 §12; Matroska
/// files written before version 4 may hold this shape).
fn matroska_three_letter_region(s: &str) -> Option<(&str, &str)> {
    let (three, two) = s.split_once('-')?;
    if three.len() == 3
        && two.len() == 2
        && three.chars().all(|c| c.is_ascii_alphabetic())
        && two.chars().all(|c| c.is_ascii_alphabetic())
    {
        Some((three, two))
    } else {
        None
    }
}

/// Converts an old three-letter (ISO 639-2) field — MP4's media header,
/// Matroska's old `Language` element, ID3's `TLAN` and the language of
/// `COMM`/`USLT`, most `ffprobe` output, and free-text fields other tools
/// write a three-letter code into (Vorbis `LANGUAGE`) — into a canonical
/// BCP 47 tag (LANG-002). Returns `None` for a value that is not
/// recognised at all; the caller then stores `und` and SHOULD keep the
/// original text alongside, rather than this function guessing.
///
/// Every language value read from a file, a tag or another system should
/// go through this function rather than straight into [`canonicalise`] —
/// [`canonicalise`] alone is for a value already known to be a BCP 47 tag
/// (one a person typed into a tag field), not one that might still be an
/// old three-letter code.
pub fn from_legacy_three_letter(input: &str) -> Option<LanguageTag> {
    // Fixed-width fields are padded with trailing null characters; strip
    // those first, then the usual whitespace. If a null remains INSIDE
    // the value, the field held several values (ID3v2.4 separates them
    // with a null) — the first is the primary language, so split there
    // and read only that.
    let without_trailing_nulls = input.trim_end_matches('\u{0}');
    let mut s = trim_lang_whitespace(without_trailing_nulls);
    if let Some((first, _rest)) = s.split_once('\u{0}') {
        s = trim_lang_whitespace(first);
    }

    // ID3's own "language not known" marker, any case.
    if s.len() == 3 && s.eq_ignore_ascii_case("xxx") {
        return Some(canonicalise("und"));
    }

    // Matroska's pre-v4 "three-letter code + country" shape — but only
    // when the three letters are not themselves an already-registered
    // language subtag. `fre-ca` (`fre` is not a subtag on its own — it
    // only means anything once converted) is this shape; `und-GB` and
    // `yue-HK` are not — `und` and `yue` are already valid BCP 47 primary
    // languages in their own right, so the whole value is read as an
    // ordinary tag instead (step 4 below), giving `und-GB` and `yue-HK`
    // rather than incorrectly refusing `und-GB` for looking like it
    // combines the "not known" marker with a region.
    if let Some((three, two)) = matroska_three_letter_region(s) {
        if !data::data().languages.contains(&three.to_ascii_lowercase()) {
            let base = from_legacy_three_letter(three)?;
            if base.kind != TagKind::Ordinary || base.tag == "und" {
                return None;
            }
            return Some(canonicalise(&format!("{}-{}", base.tag, two)));
        }
    }

    if s.len() == 3 && s.chars().all(|c| c.is_ascii_alphabetic()) {
        let data = data::data();
        let lower = s.to_ascii_lowercase();
        if let Some(mapped) = data.iso639_2.get(&lower) {
            return Some(canonicalise(mapped));
        }
        // Not a mapped 639-2 code, but possibly a genuine ISO 639-3 code
        // (e.g. `yue`, `cmn`) or a local-use code (`qaa`-`qtz`) — those
        // are already valid BCP 47 language subtags in their own right.
        if data.languages.contains(&lower) || data::in_ranges(&lower, &data.language_ranges) {
            return Some(canonicalise(&lower));
        }
        return None; // unrecognised — LANG-002 step 5
    }

    // Two letters or longer and not the shapes above: canonicalise it as
    // an ordinary tag directly.
    let t = canonicalise(s);
    if t.kind == TagKind::Malformed {
        None
    } else {
        Some(t)
    }
}

/// Converts an operating-system locale name (`en_US.UTF-8`,
/// `sr_RS@latin`) into a canonical BCP 47 tag (LANG-004). `C` and
/// `POSIX` mean "no language" and return `None`, as does anything that
/// still fails to canonicalise once converted.
pub fn from_posix_locale(input: &str) -> Option<LanguageTag> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == "C" || trimmed == "POSIX" {
        return None;
    }

    // The modifier (after `@`) is split off before the character set is
    // dropped, because a modifier can appear after the codeset
    // (`uz_UZ.UTF-8@cyrillic`) and must not be swallowed by it.
    let (mut base, modifier) = match trimmed.split_once('@') {
        Some((b, m)) => (b.to_string(), Some(m)),
        None => (trimmed.to_string(), None),
    };
    if let Some((before_charset, _)) = base.split_once('.') {
        base = before_charset.to_string();
    }
    let base = base.replace('_', "-");

    let with_script = match modifier {
        Some("latin") => insert_script(&base, "Latn"),
        Some("cyrillic") => insert_script(&base, "Cyrl"),
        // Any other modifier is dropped (and SHOULD be reported —
        // LANG-004 step 4; this function has no reporting channel for a
        // modifier that was never part of a tag, so a caller wanting
        // that note needs to inspect the raw locale name itself).
        _ => base,
    };

    let t = canonicalise(&with_script);
    if t.kind == TagKind::Malformed {
        None
    } else {
        Some(t)
    }
}

/// Inserts `script` as the second hyphen-separated part of `base` — right
/// after the language, before anything else — matching LANG-004 step 4's
/// "placed after the language".
fn insert_script(base: &str, script: &str) -> String {
    let mut parts: Vec<&str> = base.split('-').collect();
    if parts.is_empty() {
        return base.to_string();
    }
    parts.insert(1, script);
    parts.join("-")
}

/// Which of the two ISO 639-2 forms is wanted (TRACK-070). The two forms
/// differ for twenty languages, so picking the wrong one is a real error:
/// Matroska's old `Language` element wants the bibliographic form,
/// MP4/MOV's media header and ID3 want the terminology form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Iso639Form {
    /// Matroska's old `Language` element (`ger`, `fre`, `chi`).
    Bibliographic,
    /// MP4/MOV's media header and ID3 (`deu`, `fra`, `zho`).
    Terminology,
}

/// Both ISO 639-2 forms at once (TRACK-070's writing rule): the
/// bibliographic form for Matroska's old `Language` element, the
/// terminology form for MP4/MOV's media header and ID3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Iso639Write {
    pub bibliographic: String,
    pub terminology: String,
}

/// The ISO 639-2 code for `tag`'s primary language, in the requested
/// form (TRACK-070). Returns `"und".to_string()` when the tag is not
/// [`TagKind::Ordinary`] (a grandfathered, private-use or malformed value
/// has no primary language to look up at all), or when its primary
/// language has no ISO 639-2 code (most ISO 639-3 languages don't) —
/// never a guess. A primary language in the ISO 639-2 local-use range
/// (`qaa`-`qtz`) is written as itself, in both forms, because nobody but
/// the two parties using a local-use code knows what a registered
/// replacement would even mean.
///
/// Returns an owned `String` rather than `&'static str`: the local-use
/// case above echoes back a subtag that came from the caller's own `tag`,
/// which this function has no way to borrow for `'static`.
pub fn iso639_2_code(tag: &LanguageTag, form: Iso639Form) -> String {
    let Some(language) = tag.language.as_deref().filter(|_| tag.is_ordinary()) else {
        return "und".to_string();
    };
    let data = data::data();
    if let Some(codes) = data.iso639_2_for_language.get(language) {
        return match form {
            Iso639Form::Bibliographic => codes.b.clone(),
            Iso639Form::Terminology => codes.t.clone(),
        };
    }
    if data.is_iso639_2_local_use(language) {
        return language.to_string();
    }
    "und".to_string()
}

/// Both ISO 639-2 forms for `tag`'s primary language at once — the
/// function a container writer actually wants, since every format in
/// TRACK-070's table needs exactly one of the two forms and knowing which
/// language it is means both are equally cheap to work out. Built on
/// [`iso639_2_code`]; see that function for the exact rule.
pub fn iso639_2_write(tag: &LanguageTag) -> Iso639Write {
    Iso639Write {
        bibliographic: iso639_2_code(tag, Iso639Form::Bibliographic),
        terminology: iso639_2_code(tag, Iso639Form::Terminology),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso639_2_code_de_differs_between_forms() {
        let de = canonicalise("de");
        assert_eq!(iso639_2_code(&de, Iso639Form::Bibliographic), "ger");
        assert_eq!(iso639_2_code(&de, Iso639Form::Terminology), "deu");
    }

    #[test]
    fn iso639_2_code_bo_differs_between_forms() {
        let bo = canonicalise("bo");
        assert_eq!(iso639_2_code(&bo, Iso639Form::Bibliographic), "tib");
        assert_eq!(iso639_2_code(&bo, Iso639Form::Terminology), "bod");
    }

    #[test]
    fn iso639_2_code_for_a_language_with_no_639_2_code_is_und() {
        // "yue" (Cantonese) is a genuine ISO 639-3 / BCP 47 subtag with no
        // ISO 639-2 equivalent at all.
        let yue = canonicalise("yue");
        assert!(yue.is_ordinary());
        assert_eq!(iso639_2_code(&yue, Iso639Form::Bibliographic), "und");
        assert_eq!(iso639_2_code(&yue, Iso639Form::Terminology), "und");
    }

    #[test]
    fn iso639_2_code_for_a_non_ordinary_tag_is_und() {
        let malformed = canonicalise("English");
        assert!(malformed.is_malformed());
        assert_eq!(iso639_2_code(&malformed, Iso639Form::Bibliographic), "und");

        let private_use = canonicalise("x-private");
        assert_eq!(private_use.kind, TagKind::PrivateUse);
        assert_eq!(iso639_2_code(&private_use, Iso639Form::Terminology), "und");

        let grandfathered = canonicalise("cel-gaulish");
        assert_eq!(grandfathered.kind, TagKind::Grandfathered);
        assert_eq!(
            iso639_2_code(&grandfathered, Iso639Form::Terminology),
            "und"
        );
    }

    #[test]
    fn unregistered_region_is_reported_but_kept() {
        let t = canonicalise("en-JJ");
        assert!(t.is_ordinary());
        assert_eq!(t.tag, "en-JJ");
        assert!(t.notes.contains(&TagNote::UnregisteredSubtag {
            subtag: "JJ".to_string()
        }));
    }

    #[test]
    fn deprecated_region_with_no_replacement_is_reported_but_kept() {
        // CS = Serbia and Montenegro, split into two countries with no
        // single successor.
        let t = canonicalise("sr-Cyrl-CS");
        assert!(t.is_ordinary());
        assert_eq!(t.tag, "sr-Cyrl-CS");
        assert!(t.notes.contains(&TagNote::DeprecatedNoReplacement {
            subtag: "CS".to_string()
        }));
    }

    #[test]
    fn a_replaced_subtag_is_reported() {
        let t = canonicalise("iw");
        assert_eq!(t.tag, "he");
        assert!(t.notes.contains(&TagNote::SubtagReplaced {
            from: "iw".to_string(),
            to: "he".to_string()
        }));
    }

    #[test]
    fn an_extlang_fold_is_reported() {
        let t = canonicalise("zh-yue-HK");
        assert_eq!(t.tag, "yue-HK");
        assert!(t.notes.contains(&TagNote::SubtagReplaced {
            from: "zh".to_string(),
            to: "yue".to_string()
        }));
    }

    #[test]
    fn a_fully_registered_tag_has_no_notes() {
        let t = canonicalise("en-GB");
        assert!(t.notes.is_empty());
    }

    #[test]
    fn malformed_keeps_trimmed_text() {
        let t = canonicalise("  en_US  ");
        assert!(t.is_malformed());
        assert_eq!(t.tag, "en_US");
    }

    #[test]
    fn canonicalise_never_panics_on_arbitrary_bytes() {
        // A cheap fuzz-style smoke test: canonicalise must always return
        // something (never panic), whatever it is handed.
        let inputs = [
            "",
            " ",
            "-",
            "--",
            "a-b-c-d-e-f-g-h-i-j",
            "x",
            "x-",
            "-x",
            "en\u{0}GB",
            "🎵",
            "en-a",
            "en-1",
            "999",
            "en-999-999",
        ];
        for input in inputs {
            let _ = canonicalise(input);
        }
    }
}
