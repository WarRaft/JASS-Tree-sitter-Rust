#pragma description Warcraft III Region File (.w3r)
#pragma endian little
#pragma array_limit 999999

// ── war3map.w3r ─────────────────────────────────────────────────────
//
// Stores map regions (areas / rects) used by triggers, weather effects
// and ambient sounds.  Each region has a bounding rectangle, a display
// name, a sequential number, an optional weather rawcode, an optional
// ambient-sound name, and a minimap colour.
//
// Reference: https://xgm.guru/p/wc3/w3-file-format

import std.io;
import std.mem;
import std.string;
import std.core;

// ── Header ──────────────────────────────────────────────────────────

u32 format @ 0x00 [[comment("Format version (5 = TFT)")]];
u32 count  @ 0x04 [[comment("Number of regions")]];

// ── Types ───────────────────────────────────────────────────────────

struct Color {
    u8 b [[color("0000FF")]];
    u8 g [[color("00FF00")]];
    u8 r [[color("FF0000")]];
    u8 a [[color("000000")]];
} [[static, color(std::format("{:02X}{:02X}{:02X}", r, g, b))]];

struct Region {
    float left   [[comment("Left bound (world units)")]];
    float bottom [[comment("Bottom bound (world units)")]];
    float right  [[comment("Right bound (world units)")]];
    float top    [[comment("Top bound (world units)")]];
    char  name[] [[comment("Region display name")]];
    u32   num    [[comment("Region sequential number")]];
    char  weather_id[4] [[comment("Weather effect rawcode (4 chars, \\0\\0\\0\\0 = none)")]];
    char  ambient_sound[] [[comment("Ambient sound name (empty = none)")]];
    Color color  [[comment("Minimap display colour (BGRA)")]];
};

// ── Data ────────────────────────────────────────────────────────────

Region regions[count] @ 0x08;
