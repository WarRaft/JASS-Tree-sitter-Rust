#pragma description Warcraft.doo
#pragma magic [ 57 33 64 6F ] @ 0x00
#pragma endian little
#pragma array_limit 99999

import std.io;
import std.mem;
import std.string;
import std.core;

char magic[4] @ 0x00 [[comment("File signature: W3do")]];
u32 format    @ 0x04 [[comment("Format version: RoC = 7, TFT = 8")]];
u32 subformat @ 0x08 [[comment("Sub-format version: RoC = 9 or 7, TFT = 11")]];

bool isUnit in;
u32 patch = 26 [[comment("Patch version from war3map.w3i")]];

// ─── Helper types ────────────────────────────────────────────────────────────

struct Vector {
    float x, y, z;
};

/**
 * Doodad visibility / collision flag.
 *
 * With flags 0 and 1 the destructible is no longer destructible
 * (i.e. it cannot be broken etc.).
 * If a destructible has droppable items, it must have flag 0,
 * and a clone with the droppable items should be spawned in the map script.
 */
enum Flag : u8 {
    invisible = 0 [[comment("Invisible and passable (can build and walk through)")]],
    visible   = 1 [[comment("Visible and passable (semi-transparent when a unit is behind it)")]],
    normal    = 2 [[comment("Normal destructible / doodad")]],
};

// ─── Droppable items ─────────────────────────────────────────────────────────

struct DropItem {
    char rawcode[4] [[comment("Item rawcode (0 = none); can be a random item")]];
    u32  chance     [[comment("Drop chance (percentage)")]];
};

struct Drop {
    u32      count [[comment("Number of items in this drop set")]];
    DropItem items[count];
};

// ─── Inventory / abilities / random units ────────────────────────────────────

struct InventoryItem {
    u32  slot       [[comment("Inventory slot number (0-based)")]];
    char rawcode[4] [[comment("Item rawcode; can be a random item")]];
};

struct ModifiedAbility {
    char rawcode[4] [[comment("Ability rawcode")]];
    u32  autocast   [[comment("Autocast: 1 = on, 0 = off")]];
    u32  level      [[comment("Ability level")]];
};

struct RandomUnit {
    char rawcode[4] [[comment("Object rawcode (uDNR / bDNR / iDNR = random)")]];
    u32  chance     [[comment("Chance (percentage)")]];
};

/**
 * Random rawcode encoding
 * ─────────────────────────────────────────────────────────────────────────────
 *
 * Random unit:
 *   [0] 'Y'  — random object marker
 *   [1] 'Y'  — any type
 *   [2] 'U'  — random unit
 *   [3] '/' any level, '0' level 1, '1' level 2, … ':' level 10
 *   Example: 'YYU:' = random unit level 10
 *
 * Random building:
 *   [0] 'Y'  [1] 'Y'  [2] 'B'  [3] '/'
 *   Always 'YYB/' (buildings have no levels)
 *
 * Random item:
 *   [0] 'Y'  — random object marker
 *   [1] type: 'Y' any, 'i' permanent, 'j' charged, 'k' power-up,
 *             'l' artifact, 'm' purchasable, 'n' campaign, 'o' miscellaneous
 *   [2] 'I'  — random item
 *   [3] '/' any level, '0' level 1, '1' level 2, … ':' level 10
 *   Example: 'YkI:' = random power-up item level 10
 */

// ─── Placed object (unit or doodad) ─────────────────────────────────────────

struct Item {
    char   rawcode[4] [[comment("Unit / doodad rawcode")]];
    u32    variation   [[comment("Variation (can be ignored for units)")]];
    Vector position;
    float  angle       [[comment("Rotation angle (radians)")]];
    Vector scale       [[comment("Scale (can be ignored for units)")]];
    if (patch >= 32)
        u32 skin       [[comment("Skin override")]];
    Flag   flag        [[comment("Visibility flag (can be ignored for units)")]];

    if (isUnit) {
        // ── Unit common fields ───────────────────────────────────────────
        u32   player       [[comment("Owner player number")]];
        u8    unknown1;
        u8    unknown2;
        u32   health       [[comment("Hit points (-1 = default)")]];
        u32   mana         [[comment("Mana points (-1 = default)")]];

        if (subformat >= 11) {
            u32 drop_table [[comment("Random item table index (-1 = not used)")]];
        }

        u32  drop_count    [[comment("Number of item drop sets")]];
        Drop drops[drop_count];

        s32   gold_mine    [[comment("Gold in mine (default: 12500)")]];
        float target       [[comment("Acquisition range (-1.0 = normal, -2.0 = camp)")]];
        u32   hero_level   [[comment("Hero level (1 for non-heroes and items)")]];

        if (subformat >= 11) {
            u32 strength     [[comment("Hero Strength (0 = default)")]];
            u32 agility      [[comment("Hero Agility (0 = default)")]];
            u32 intelligence [[comment("Hero Intelligence (0 = default)")]];
        }

        // ── Inventory ────────────────────────────────────────────────────
        u32             inventory_count [[comment("Number of inventory items")]];
        InventoryItem   inventory[inventory_count];

        // ── Modified abilities ───────────────────────────────────────────
        u32             ability_count [[comment("Number of modified abilities")]];
        ModifiedAbility abilities[ability_count];

        // ── Random combat unit ───────────────────────────────────────────
        s32 random_type [[comment("Random type: 0 / 1 / 2 (0 for all except uDNR/bDNR/iDNR)")]];

        if (random_type == 0) {
            u8 random_level[3] [[comment("Level (-1 = any); 24-bit value")]];
            u8 item_class      [[comment("0 = any, 1 = permanent, 2 = charged, 3 = power-up, 4 = artifact, 5 = purchasable, 6 = campaign, 7 = miscellaneous")]];
        } else if (random_type == 1) {
            u32 random_group    [[comment("Random group number (from war3map.w3i)")]];
            u32 random_position [[comment("Column position number")]];
        } else if (random_type == 2) {
            u32        random_unit_count [[comment("Number of random unit entries")]];
            RandomUnit random_units[random_unit_count];
        }

        s32 custom_color [[comment("Custom player color (-1 = none, 0 = red, 1 = blue, … 11 = brown)")]];
        s32 waygate      [[comment("Waygate target region in war3map.w3r (-1 = none)")]];
        u32 object_id    [[comment("Unique object ID")]];

    } else {
        // ── Doodad / destructible ────────────────────────────────────────
        u8 health [[comment("Health (patch ≥ 32: 0–255 where 255 = 100%; otherwise 0%–100%)")]];

        if (format == 8) {
            s32  drop_index [[comment("Random item table index (-1 = none)")]];
            u32  drop_count [[comment("Number of item drop sets")]];
            Drop drops[drop_count];
        }

        u32 num [[comment("Doodad creation number")]];
    }
};

// ─── Cliff decorations ──────────────────────────────────────────────────────

struct Cliff {
    char rawcode[4] [[comment("Cliff doodad rawcode")]];
    u32  variation   [[comment("Variation")]];
    u32  x           [[comment("Tile X coordinate (w3e grid)")]];
    u32  y           [[comment("Tile Y coordinate (w3e grid)")]];
};

// ─── Root structure ─────────────────────────────────────────────────────────

struct Data {
    u32  item_count [[comment("Number of placed objects")]];
    Item items[item_count];

    if (!isUnit) {
        u32   cliff_format [[comment("Cliff decorations format version")]];
        u32   cliff_count  [[comment("Number of cliff decorations")]];
        Cliff cliffs[cliff_count];
    }
};

Data data @ 0x0C;
