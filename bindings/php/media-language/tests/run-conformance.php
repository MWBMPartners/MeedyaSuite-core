<?php

/**
 * bindings/php/media-language/tests/run-conformance.php
 *
 * Runs every conformance case in tests/fixtures/bcp47-language-policy-v1.json
 * against the PHP implementation of MWBM-MEDIA-LANG
 * (../MediaLanguagePolicy.php). Plain PHP, no PHPUnit and no other
 * dependency, on purpose: the three consumer applications (iHymns,
 * iLyricsDB, NetPLAYERapp) have no test framework of their own, and this
 * file is meant to run the same way in all of them, with `php` alone.
 *
 * Usage:
 *   php run-conformance.php [--fixtures <path>] [--data <path>]
 *
 * With no arguments, both paths default to the copies inside THIS
 * repository (MeedyaSuite-core), found relative to this script's own
 * location. A repository that has taken a copy of the policy (section 8.3)
 * passes its own paths instead, for example:
 *
 *   php run-conformance.php \
 *     --fixtures docs/standards/tests/bcp47-language-policy-v1.json \
 *     --data docs/standards/data/bcp47-language-data-v1.json
 *
 * Exit code is 0 only when every case in every section passed AND the
 * number of cases actually run matches the number declared in the fixture
 * file - a section silently skipped (a typo'd array key, for instance)
 * would otherwise still print "0 failed" and look like a clean pass.
 *
 * Copyright (c) 2026 MeedyaSuite
 * Licensed under the MIT License. See LICENSE file in the project root.
 */

declare(strict_types=1);

require __DIR__ . '/../MediaLanguagePolicy.php';

use Mwbm\MediaLanguage\Policy;
use Mwbm\MediaLanguage\SubtitleMode;

/**
 * The sections of the fixture file, in the order the policy document lists
 * them (section 8.1's table). Kept as a named list, rather than just
 * iterating whatever keys the JSON happens to have, so a section renamed
 * or removed from the fixture file causes an immediate, loud failure here
 * rather than a quietly smaller test run.
 */
const EXPECTED_SECTIONS = [
    'canonicalise',
    'legacy_three_letter',
    'iso639_2_write',
    'posix_locale',
    'sidecar_name',
    'canonical_order',
    'track_order',
    'presentation_order',
    'subtitle_menu',
    'label',
    'match',
    'auto_select_audio',
    'auto_select_subtitle',
];

/**
 * @param array<int, string> $argv
 * @return array{fixtures: string, data: string}
 */
function parseArguments(array $argv): array
{
    $repoRoot = realpath(__DIR__ . '/../../../../');
    if ($repoRoot === false) {
        fwrite(STDERR, "Could not resolve the repository root from " . __DIR__ . "\n");
        exit(1);
    }
    $fixtures = $repoRoot . '/tests/fixtures/bcp47-language-policy-v1.json';
    $data = $repoRoot . '/docs/standards/data/bcp47-language-data-v1.json';

    $count = count($argv);
    for ($i = 1; $i < $count; $i++) {
        if ($argv[$i] === '--fixtures' && $i + 1 < $count) {
            $fixtures = $argv[++$i];
        } elseif ($argv[$i] === '--data' && $i + 1 < $count) {
            $data = $argv[++$i];
        } else {
            fwrite(STDERR, "Unrecognised argument: {$argv[$i]}\n");
            fwrite(STDERR, "Usage: php run-conformance.php [--fixtures <path>] [--data <path>]\n");
            exit(1);
        }
    }
    return ['fixtures' => $fixtures, 'data' => $data];
}

/** @var list<string> $failures */
$failures = [];
$casesRun = 0;
$checksRun = 0;

/**
 * Records one pass/fail check. $extra names which run this is (for example
 * "reversed"), so a failure that only shows up with the tracks reversed is
 * distinguishable in the printed output from the normal-order failure.
 */
function check(string $caseId, mixed $actual, mixed $expected, string $extra = ''): void
{
    global $failures, $checksRun;
    $checksRun++;
    if ($actual !== $expected) {
        $label = $extra === '' ? $caseId : "{$caseId} ({$extra})";
        $failures[] = sprintf(
            "%s: expected %s, got %s",
            $label,
            json_encode($expected, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE),
            json_encode($actual, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE)
        );
    }
}

$args = parseArguments($argv);

if (!is_file($args['fixtures'])) {
    fwrite(STDERR, "No fixture file at {$args['fixtures']}\n");
    exit(1);
}
$fixtureRaw = file_get_contents($args['fixtures']);
if ($fixtureRaw === false) {
    fwrite(STDERR, "Could not read {$args['fixtures']}\n");
    exit(1);
}
$fixtures = json_decode($fixtureRaw, true);
if (!is_array($fixtures)) {
    fwrite(STDERR, "{$args['fixtures']} is not valid JSON\n");
    exit(1);
}

foreach (EXPECTED_SECTIONS as $section) {
    if (!isset($fixtures[$section]) || !is_array($fixtures[$section])) {
        fwrite(STDERR, "The fixture file has no '{$section}' section (or it is not an array).\n");
        exit(1);
    }
}
$declaredTotal = 0;
foreach (EXPECTED_SECTIONS as $section) {
    $declaredTotal += count($fixtures[$section]);
}

try {
    Policy::loadData($args['data']);
} catch (\RuntimeException $e) {
    fwrite(STDERR, "Could not load reference data: " . $e->getMessage() . "\n");
    exit(1);
}

// --- canonicalise (LANG-001, LANG-026) --------------------------------
foreach ($fixtures['canonicalise'] as $case) {
    $casesRun++;
    $tag = Policy::canonicalise($case['input']);
    $actual = [$tag->isMalformed() ? null : $tag->tag, $tag->kind->value];
    check($case['id'], $actual, [$case['expected'], $case['kind']]);

    // Stability check (LANG-001, revision 2): canonical form must be a
    // fixed point - canonicalising an already-canonical tag has to return
    // it unchanged. Run for every case whose expected value is a real tag
    // (skipped for the malformed cases, which have no tag to re-check).
    // This is an extra verification of the SAME case, not a new declared
    // case, so it adds to $checksRun but not to $casesRun - the same
    // convention as the reversed AUTO-010 re-checks below.
    if ($case['expected'] !== null) {
        $restabilised = Policy::canonicalise($case['expected']);
        check($case['id'], $restabilised->tag, $case['expected'], 'stability');
    }
}

// --- legacy_three_letter (LANG-002, LANG-003) --------------------------
foreach ($fixtures['legacy_three_letter'] as $case) {
    $casesRun++;
    check($case['id'], Policy::fromLegacyThreeLetter($case['input']), $case['expected']);
}

// --- iso639_2_write (TRACK-070) ------------------------------------------
foreach ($fixtures['iso639_2_write'] as $case) {
    $casesRun++;
    check($case['id'], Policy::iso6392CodesForWriting($case['input']), $case['expected']);
}

// --- posix_locale (LANG-004) --------------------------------------------
foreach ($fixtures['posix_locale'] as $case) {
    $casesRun++;
    check($case['id'], Policy::fromPosixLocale($case['input']), $case['expected']);
}

// --- sidecar_name (TEXT-030) ---------------------------------------------
foreach ($fixtures['sidecar_name'] as $case) {
    $casesRun++;
    if ($case['mode'] === 'build') {
        $name = Policy::buildSidecarName(
            $case['stem'],
            $case['tag'],
            $case['roles'],
            $case['extension'],
            $case['number']
        );
        check($case['id'], $name, $case['expected']);
    } else {
        $parsed = Policy::parseSidecarName($case['stem'], $case['filename']);
        check($case['id'], $parsed, $case['expected']);
    }
}

// --- canonical_order (LANG-010 to LANG-027) -----------------------------
foreach ($fixtures['canonical_order'] as $case) {
    $casesRun++;
    $items = array_map(
        static fn (array $item): array => [
            'id' => $item['id'] ?? $item['tag'],
            'tag' => $item['tag'],
            'original' => $item['original'] ?? false,
        ],
        $case['items']
    );
    $ordered = Policy::sortCanonicalOrder($items);
    $ids = array_map(static fn (array $i): string => $i['id'], $ordered);
    check($case['id'], $ids, $case['expected']);
}

// --- track_order (TRACK-050, TRACK-060) ---------------------------------
foreach ($fixtures['track_order'] as $case) {
    $casesRun++;
    $ordered = Policy::sortTrackOrder($case['tracks']);
    $ids = array_map(static fn (array $t): string => $t['id'], $ordered);
    check($case['id'], $ids, $case['expected']);
}

/**
 * Builds the $groupCompare callable the brief specifies: compare
 * collation_keys as plain strings (the library itself then breaks a tie by
 * primary subtag, per UI-040's "Groups with the same name are ordered by
 * primary language code" - see Policy::sortPresentation()'s doc comment).
 *
 * @param array<string, string> $collationKeys
 * @return callable(string, string): int
 */
function presentationGroupCompare(array $collationKeys): callable
{
    return static function (string $a, string $b) use ($collationKeys): int {
        return strcmp($collationKeys[$a] ?? $a, $collationKeys[$b] ?? $b);
    };
}

// --- presentation_order (UI-020 to UI-050) ------------------------------
foreach ($fixtures['presentation_order'] as $case) {
    $casesRun++;
    $ordered = Policy::sortPresentation(
        $case['items'],
        $case['preferences'],
        presentationGroupCompare($case['collation_keys']),
        $case['accessibility']
    );
    $ids = array_map(static fn (array $i): string => $i['id'], $ordered);
    check($case['id'], $ids, $case['expected']);
}

// --- subtitle_menu (UI-060) ---------------------------------------------
foreach ($fixtures['subtitle_menu'] as $case) {
    $casesRun++;
    $ordered = Policy::sortSubtitleMenu(
        $case['items'],
        $case['preferences'],
        presentationGroupCompare($case['collation_keys']),
        $case['accessibility']
    );
    $ids = array_map(static fn (?array $i): string => $i === null ? 'off' : $i['id'], $ordered);
    check($case['id'], $ids, $case['expected']);
}

// --- label (UI-070) ------------------------------------------------------
foreach ($fixtures['label'] as $case) {
    $casesRun++;
    $label = Policy::buildLabel(
        $case['type'],
        $case['language_name'],
        $case['roles'],
        $case['role_names'],
        $case['channels']
    );
    check($case['id'], $label, $case['expected']);
}

// --- match (MATCH-010 to MATCH-040) --------------------------------------
foreach ($fixtures['match'] as $case) {
    $casesRun++;
    $result = Policy::matchTags($case['preference'], $case['candidate']);
    $actual = ['level' => $result->level->value, 'distance' => $result->distance];
    check($case['id'], $actual, $case['expected']);
}

// --- auto_select_audio (AUTO-010, AUTO-020, AUTO-040) --------------------
foreach ($fixtures['auto_select_audio'] as $case) {
    $casesRun++;
    $chosen = Policy::selectAudioTrack($case['tracks'], $case['preferences'], $case['accessibility']);
    check($case['id'], $chosen, $case['expected']);

    // AUTO-010: the same answer whatever order the tracks are listed in.
    $reversedChosen = Policy::selectAudioTrack(
        array_reverse($case['tracks']),
        $case['preferences'],
        $case['accessibility']
    );
    check($case['id'], $reversedChosen, $case['expected'], 'reversed');
}

// --- auto_select_subtitle (AUTO-010, AUTO-030, AUTO-040) -----------------
foreach ($fixtures['auto_select_subtitle'] as $case) {
    $casesRun++;
    $mode = SubtitleMode::from($case['mode']);
    $chosen = Policy::selectSubtitleTrack(
        $case['tracks'],
        $case['audio'],
        $case['preferences'],
        $mode,
        $case['accessibility']
    );
    check($case['id'], $chosen, $case['expected']);

    $reversedChosen = Policy::selectSubtitleTrack(
        array_reverse($case['tracks']),
        $case['audio'],
        $case['preferences'],
        $mode,
        $case['accessibility']
    );
    check($case['id'], $reversedChosen, $case['expected'], 'reversed');
}

// -------------------------------------------------------------------------

if ($failures !== []) {
    fwrite(STDERR, "Failures:\n");
    foreach ($failures as $failure) {
        fwrite(STDERR, "  - {$failure}\n");
    }
}

if ($casesRun !== $declaredTotal) {
    fwrite(
        STDERR,
        "Case-count mismatch: ran {$casesRun} cases but the fixture file declares "
        . "{$declaredTotal} across its " . count(EXPECTED_SECTIONS) . " sections. "
        . "A section was probably skipped.\n"
    );
    exit(1);
}

printf(
    "MWBM-MEDIA-LANG PHP conformance: %d/%d cases run (%d checks including reversed "
    . "AUTO-010 re-checks and canonical-form stability re-checks), %d failure(s).\n",
    $casesRun,
    $declaredTotal,
    $checksRun,
    count($failures)
);

exit($failures === [] ? 0 : 1);
