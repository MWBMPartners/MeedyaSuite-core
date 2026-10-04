#!/usr/bin/env python3
# Copyright (c) 2026 MeedyaSuite
# Licensed under the MIT License. See LICENSE file in the project root.
#
# Makes the small, real M4A files the `tag_io` tests read (issue #102).
# =====================================================================
#
# Why real files: the tests check what a save through lofty does to atoms
# OTHER programs wrote - their exact types, sizes and names - so the atoms
# have to be written by another program, not by the library under test.
# Each file starts as a 0.2-second silent AAC file made by ffmpeg; mutagen
# (the Python tagging library) then writes the atoms. The files at the end
# of this script, for the check of the WHOLE saved file (audio, chunk
# offsets, chapters), are half a second of a tone instead, and one of them
# is fragmented and tagged by ffmpeg itself. A few shapes mutagen
# will not write (two atoms of the same name, a data type or locale it
# does not use) are made by editing the `ilst` atom's bytes directly, with
# the sizes of the atoms around it corrected; every file was then read back
# with mutagen and ffprobe to check it is a valid M4A file.
#
# Made with ffmpeg 9.0.1 and mutagen 1.48.1. Run from this folder:
#     python3 make_m4a_fixtures.py
# With those versions a re-run gives every file byte for byte (checked
# when the tone files were added). ffmpeg's output depends on its version,
# so re-running with another version gives different bytes; the tests only
# need the atoms described below, not these exact bytes.

import os
import struct
import subprocess

from mutagen.mp4 import MP4, AtomDataType, MP4Cover, MP4FreeForm



def tiny_jpeg():
    """A real 8-by-8 red JPEG image, made by ffmpeg, for the cover art."""
    subprocess.run(
        ["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "color=c=red:s=8x8",
         "-frames:v", "1", "-y", "cover.jpg"],
        check=True,
    )
    data = open("cover.jpg", "rb").read()
    os.remove("cover.jpg")
    return data


JPEG = tiny_jpeg()


def base(name):
    """A fresh untagged file: 0.2 s of silence, AAC, as ffmpeg writes it."""
    subprocess.run(
        ["ffmpeg", "-v", "error", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono",
         "-t", "0.2", "-c:a", "aac", "-y", name],
        check=True,
    )
    return name


def tagged(name, tags):
    """`name`, made by ffmpeg and then given `tags` by mutagen."""
    base(name)
    m = MP4(name)
    for key, value in tags.items():
        m[key] = value
    m.save()


# ------------------------------------------------------------------
# Editing the `ilst` atom's bytes, for the shapes mutagen will not write
# ------------------------------------------------------------------

def _atoms(b, start, end):
    """(offset, size, name) of each atom between `start` and `end`."""
    out, pos = [], start
    while pos + 8 <= end:
        size, name = struct.unpack(">I4s", b[pos:pos + 8])
        out.append((pos, size, name))
        pos += size
    return out


def edit_ilst(name, change):
    """Replace the file's `ilst` children with `change(children)`, where
    `children` is a list of each child atom's bytes, and correct the sizes
    of `ilst`, `meta`, `udta` and `moov`. `moov` must come after `mdat`
    (ffmpeg's default), so no sample offset moves."""
    b = bytearray(open(name, "rb").read())
    top = _atoms(b, 0, len(b))
    names = [n for (_, _, n) in top]
    assert names.index(b"moov") > names.index(b"mdat"), "moov must follow mdat"
    moov = next(a for a in top if a[2] == b"moov")
    udta = next(a for a in _atoms(b, moov[0] + 8, moov[0] + moov[1]) if a[2] == b"udta")
    meta = next(a for a in _atoms(b, udta[0] + 8, udta[0] + udta[1]) if a[2] == b"meta")
    ilst = next(a for a in _atoms(b, meta[0] + 12, meta[0] + meta[1]) if a[2] == b"ilst")
    children = [bytes(b[p:p + s]) for (p, s, _) in _atoms(b, ilst[0] + 8, ilst[0] + ilst[1])]
    body = b"".join(change(children))
    new_ilst = struct.pack(">I4s", 8 + len(body), b"ilst") + body
    delta = len(new_ilst) - ilst[1]
    b[ilst[0]:ilst[0] + ilst[1]] = new_ilst
    for (pos, size, _) in (meta, udta, moov):
        b[pos:pos + 4] = struct.pack(">I", size + delta)
    open(name, "wb").write(bytes(b))


def data_atom(type_code, value, locale=0):
    """One `data` child: type set 0, 24-bit type, 4-byte locale, value."""
    return struct.pack(">I4sII", 16 + len(value), b"data", type_code, locale) + value


def child_named(children, fourcc, freeform_name=None):
    for i, c in enumerate(children):
        if c[4:8] == fourcc and (freeform_name is None or freeform_name in c):
            return i
    raise KeyError(fourcc)


# ------------------------------------------------------------------
# The files
# ------------------------------------------------------------------

# The stand-in review of revision 8's file: flags written as numbers,
# freeform atoms whose names are not spelled the way lofty spells them.
tagged("flags-and-freeform.m4a", {
    "\xa9nam": ["Orig"],
    "pgap": False,
    "hdvd": [0],
    "shwm": [0],
    "cpil": False,
    "----:com.apple.iTunes:Mood": [MP4FreeForm(b"calm", AtomDataType.UTF8)],
    "----:com.apple.iTunes:REPLAYGAIN_TRACK_GAIN": [MP4FreeForm(b"-3.00 dB", AtomDataType.UTF8)],
    "----:com.apple.iTunes:isrc": [MP4FreeForm(b"GBAAA0000001", AtomDataType.UTF8)],
})

# Cover art whose second value carries the text data type (1) instead of
# an image type: lofty drops the whole `covr` atom when it reads it.
tagged("two-cover-types.m4a", {
    "covr": [MP4Cover(JPEG, imageformat=MP4Cover.FORMAT_JPEG),
             MP4Cover(b"second-image-bytes", imageformat=AtomDataType.UTF8)],
})

# One value per atom, every atom in a form lofty writes back unchanged.
tagged("one-value-each.m4a", {
    "\xa9nam": ["Orig"],
    "\xa9ART": ["Artist"],
    "\xa9alb": ["Album"],
    "aART": ["Album Artist"],
    "\xa9gen": ["Pop"],
    "\xa9day": ["2020-05-01"],
    "cprt": ["(c) 2020"],
    "soar": ["Artist, The"],
    "trkn": [(3, 10)],
    "cpil": True,
    "covr": [MP4Cover(JPEG, imageformat=MP4Cover.FORMAT_JPEG)],
    "----:com.apple.iTunes:MOOD": [MP4FreeForm(b"calm", AtomDataType.UTF8)],
    "----:com.apple.iTunes:LANGUAGE": [MP4FreeForm(b"en", AtomDataType.UTF8),
                                       MP4FreeForm(b"fr", AtomDataType.UTF8)],
})

# Two artists in one atom (finding 5: replacing it is allowed).
tagged("two-artists.m4a", {
    "\xa9ART": ["Alice", "Bob"],
    "\xa9alb": ["Album"],
})

# One language, for a registry write aimed at the language atom.
tagged("language-en.m4a", {
    "\xa9nam": ["Orig"],
    "----:com.apple.iTunes:LANGUAGE": [MP4FreeForm(b"en", AtomDataType.UTF8)],
})

# A file as iTunes and Apple Music leave it: whole numbers in one, two or
# four bytes, a six-byte disc number, a gapless flag.
tagged("itunes-style.m4a", {
    "\xa9nam": ["Orig"],
    "\xa9ART": ["Artist"],
    "trkn": [(3, 10)],
    "disk": [(1, 2)],
    "pgap": False,
    "stik": [1],
    "rtng": [1],
    "tmpo": [120],
    "akID": [0],
    "cnID": [123456789],
})

# Two separate ISRC atoms, the second holding a newer value (mutagen keeps
# one atom per name, so the second is added by editing the bytes).
tagged("two-isrc-atoms.m4a", {
    "\xa9nam": ["Orig"],
    "----:com.apple.iTunes:ISRC": [MP4FreeForm(b"OLD000000001", AtomDataType.UTF8)],
})


def add_second_isrc(children):
    i = child_named(children, b"----", b"ISRC")
    return children[:i + 1] + [children[i].replace(b"OLD000000001", b"NEW000000002")] + children[i + 1:]


edit_ilst("two-isrc-atoms.m4a", add_second_isrc)

# A track-number atom holding two values (track 1 of 10, then 2 of 10).
tagged("two-track-numbers.m4a", {"\xa9nam": ["Orig"], "trkn": [(1, 10)]})


def two_track_numbers(children):
    i = child_named(children, b"trkn")
    body = data_atom(0, b"\x00\x00\x00\x01\x00\x0a\x00\x00") + data_atom(0, b"\x00\x00\x00\x02\x00\x0a\x00\x00")
    children[i] = struct.pack(">I4s", 8 + len(body), b"trkn") + body
    return children


edit_ilst("two-track-numbers.m4a", two_track_numbers)

# The compilation flag stored as an UNSIGNED whole number (type 22) rather
# than the usual signed one (21). lofty reads it as a flag and writes it
# back as type 21 with the same byte: only the type changes.
tagged("compilation-typed-unsigned.m4a", {"\xa9nam": ["Orig"], "cpil": True})


def cpil_unsigned(children):
    i = child_named(children, b"cpil")
    c = bytearray(children[i])
    assert c[8 + 4:8 + 8] == b"data" and c[8 + 11] == 21
    c[8 + 11] = 22
    children[i] = bytes(c)
    return children


edit_ilst("compilation-typed-unsigned.m4a", cpil_unsigned)

# The encoder atom (`©too`, written by ffmpeg) with a non-zero locale.
# lofty writes every locale back as zero: only the locale changes.
tagged("encoder-with-locale.m4a", {"\xa9nam": ["Orig"]})


def encoder_locale(children):
    i = child_named(children, b"\xa9too")
    c = bytearray(children[i])
    assert c[8 + 4:8 + 8] == b"data" and c[8 + 12:8 + 16] == b"\x00\x00\x00\x00"
    c[8 + 12:8 + 16] = b"\x00\x00\x00\x01"
    children[i] = bytes(c)
    return children


edit_ilst("encoder-with-locale.m4a", encoder_locale)


# ------------------------------------------------------------------
# Files with real audio, for the check of the WHOLE saved file (the
# stand-in review of revision 9: a save can change what lies OUTSIDE the
# tags - the audio of a fragmented file, the handler beside a missing tag
# list). Half a second of a 440 Hz tone rather than silence, so a
# misplaced sample offset decodes as noise or an error, not as more
# silence.
# ------------------------------------------------------------------

def tone(name, extra=(), seconds="0.5", metadata=None):
    """`name`: a tone made by ffmpeg, with `extra` output options."""
    cmd = ["ffmpeg", "-v", "error", "-f", "lavfi",
           "-i", f"sine=frequency=440:sample_rate=44100:duration={seconds}"]
    if metadata:
        cmd += ["-i", metadata, "-map", "0", "-map_metadata", "1", "-map_chapters", "1"]
    cmd += ["-c:a", "aac", "-b:a", "64k", *extra, "-y", name]
    subprocess.run(cmd, check=True)


# A plain file as mutagen tags one: text, a track number, one cover
# image, an ISRC. `moov` after `mdat` (ffmpeg's default), so growing
# tags move no audio.
tone("plain-tone.m4a")
m = MP4("plain-tone.m4a")
m["\xa9nam"] = ["Song"]
m["\xa9ART"] = ["Artist"]
m["\xa9alb"] = ["Album"]
m["trkn"] = [(3, 12)]
m["covr"] = [MP4Cover(JPEG, imageformat=MP4Cover.FORMAT_JPEG)]
m["----:com.apple.iTunes:ISRC"] = [MP4FreeForm(b"GBAAA1900001", AtomDataType.UTF8)]
m.save()

# Chapters as ffmpeg writes them in an M4A file: a QuickTime chapter
# track (a second, text track the audio track points to) and a Nero
# `chpl` atom in `udta`. Two layouts: `moov` after `mdat`, and `moov`
# first ("faststart"), where tags that grow past their padding move the
# audio and every chunk offset must move with it.
with open("chapters.txt", "w") as f:
    f.write(";FFMETADATA1\n"
            "[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=250\ntitle=One\n"
            "[CHAPTER]\nTIMEBASE=1/1000\nSTART=250\nEND=500\ntitle=Two\n")
for name, extra in (("chapters.m4a", ()), ("chapters-faststart.m4a", ("-movflags", "+faststart"))):
    tone(name, extra, metadata="chapters.txt")
    m = MP4(name)
    m["\xa9nam"] = ["Song"]
    m["\xa9ART"] = ["Artist"]
    m.save()
os.remove("chapters.txt")

# Fragmented, as streaming tools write it: an empty `moov` holding `mvex`,
# then several `moof` + `mdat` pairs. Tags written by ffmpeg.
tone("fragmented.m4a",
     ("-metadata", "title=Song", "-metadata", "artist=Artist",
      "-movflags", "frag_keyframe+empty_moov", "-frag_duration", "200000"),
     seconds="0.6")

# Every tag removed by mutagen (`clear()` then save): an empty `ilst`
# beside the handler, and padding. (A tag removal by lofty itself leaves
# the handler with NO `ilst` at all - the tests make that shape with
# lofty, from `plain-tone.m4a`.)
subprocess.run(["cp", "plain-tone.m4a", "mutagen-cleared.m4a"], check=True)
m = MP4("mutagen-cleared.m4a")
m.clear()
m.save()
