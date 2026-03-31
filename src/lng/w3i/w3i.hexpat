#pragma description Warcraft.w3i
#pragma endian little
#pragma array_limit 999999

import std.io;
import std.mem;
import std.string;
import std.core;

enum Format: u32 {
    RoC   = 18 [[comment("Reign of Chaos")]],
    TFT   = 25 [[comment("The Frozen Throne")]],
    V1_31 = 28 [[comment("Patch 1.31")]],
    V1_32 = 31 [[comment("Patch 1.32")]],
    V2_03 = 33 [[comment("Patch 2.0.3")]],
};

Format format @ 0x00;

struct Color {
    u8 b [[color("0000FF")]];
    u8 g [[color("00FF00")]];
    u8 r [[color("FF0000")]];
    u8 a [[color("000000")]];
} [[static, color(std::format("{:02X}{:02X}{:02X}", r, g, b))]];;

bitfield MapFlags {
    hide_minimap             : 1 [[comment("Hide minimap on preview screens")]];
    ally_priorities_edit     : 1 [[comment("Modify ally priorities")]];
    melee_map                : 1 [[comment("Map is suitable for Melee mode")]];
    custom_terrain_type      : 1 [[comment("Use custom terrain type")]];
    partial_visibility       : 1 [[comment("Masked areas are partially visible")]];
    fixed_player_settings    : 1 [[comment("Fixed player settings")]];
    custom_forces            : 1 [[comment("Custom forces")]];
    custom_techs             : 1 [[comment("Custom tech tree")]];
    custom_abilities         : 1 [[comment("Custom abilities")]];
    custom_upgrades          : 1 [[comment("Custom upgrades")]];
    unknown_flag_0x400       : 1 [[comment("Unknown")]];
    waves_steep_shore        : 1 [[comment("Show waves on steep shores")]];
    waves_shallow_shore      : 1 [[comment("Show waves on shallow shores")]];
    terrain_fog              : 1 [[comment("Terrain fog enabled")]];
    expansion_required       : 1 [[comment("Expansion required")]];
    item_classification      : 1 [[comment("Item classification")]];
    water_color_override     : 1 [[comment("Water tint color override")]];
    padding                  : 15;
};

enum PlayerType: u32 {
    Human    = 1 [[comment("Human player")]],
    Comp     = 2 [[comment("Computer player")]],
    Neutral  = 3 [[comment("Neutral player")]],
    Reserve  = 4 [[comment("Rescuable slot")]],
};

enum Race: u32 {
    Random   = 0 [[comment("Random race")]],
    Human    = 1 [[comment("Alliance")]],
    Orc      = 2 [[comment("Horde")]],
    Undead   = 3 [[comment("Undead")]],
    NightElf = 4 [[comment("Night Elves")]],
};

bitfield PlayerBool {
    Player1   : 1 [[comment("Player 1")]];
    Player2   : 1 [[comment("Player 2")]];
    Player3   : 1 [[comment("Player 3")]];
    Player4   : 1 [[comment("Player 4")]];
    Player5   : 1 [[comment("Player 5")]];
    Player6   : 1 [[comment("Player 6")]];
    Player7   : 1 [[comment("Player 7")]];
    Player8   : 1 [[comment("Player 8")]];
    Player9   : 1 [[comment("Player 9")]];
    Player10  : 1 [[comment("Player 10")]];
    Player11  : 1 [[comment("Player 11")]];
    Player12  : 1 [[comment("Player 12")]];
    Player13  : 1 [[comment("Player 13")]];
    Player14  : 1 [[comment("Player 14")]];
    Player15  : 1 [[comment("Player 15")]];
    Player16  : 1 [[comment("Player 16")]];
    Player17  : 1 [[comment("Player 17")]];
    Player18  : 1 [[comment("Player 18")]];
    Player19  : 1 [[comment("Player 19")]];
    Player20  : 1 [[comment("Player 20")]];
    Player21  : 1 [[comment("Player 21")]];
    Player22  : 1 [[comment("Player 22")]];
    Player23  : 1 [[comment("Player 23")]];
    Player24  : 1 [[comment("Player 24")]];
    Player25  : 1 [[comment("Player 25")]];
    Player26  : 1 [[comment("Player 26")]];
    Player27  : 1 [[comment("Player 27")]];
    Player28  : 1 [[comment("Player 28")]];
    Player29  : 1 [[comment("Player 29")]];
    Player30  : 1 [[comment("Player 30")]];
    Player31  : 1 [[comment("Player 31")]];
    Player32  : 1 [[comment("Player 32")]];
};

struct Point {
    float x;
    float y;
};

struct Rect {
    Point lb;
    Point rt;
    Point lt;
    Point rb;
};


struct Player {
    u32 num [[comment("Player number")]];
    PlayerType type;
    Race race;
    u32 fixed_start_position [[comment("Fixed starting position (lowest bit: 0=no, 1=yes)")]];
    char name[] [[comment("Player name")]];
    Point start_position [[comment("Starting position coordinates")]];
    PlayerBool ally_priority_low [[comment("Low ally priorities (bit per player: 0=none, 1=set)")]];
    PlayerBool ally_priority_high [[comment("High ally priorities (bit per player: 0=none, 1=set)")]];
    if (format >= 31){
        u32;
        u32;
    }
};

bitfield ClanFlags {
    allied                : 1 [[comment("Allied")]];
    shared_victory        : 1 [[comment("Shared victory")]];
    unknown_flag_0x0004   : 1 [[comment("Unknown")]];
    shared_vision         : 1 [[comment("Shared vision")]];
    shared_units          : 1 [[comment("Shared units")]];
    shared_all_units      : 1 [[comment("Shared units: all")]];
    padding               : 26;
};

struct Clan {
    ClanFlags flags;
    u32 player_mask [[comment("Player bitmask (bit per player: 0=not in force, 1=in force). Players not assigned to any force default to force 0")]];
    char name[] [[comment("Force name")]];
};


enum UpgradeStatus: u32 {
    Disabled   = 0 [[comment("Disabled")]],
    Available  = 1 [[comment("Available")]],
    Researched = 2 [[comment("Researched")]],
};

struct Upgrade {
    PlayerBool affected_players [[comment("Affected players (bit per player: 0=unaffected, 1=affected)")]];
    char rawcode[4] [[comment("Upgrade rawcode")]];
    u32 level [[comment("Upgrade level")]];
    UpgradeStatus status [[comment("Upgrade availability")]];
};

struct Tech {
    PlayerBool disabled_players [[comment("Players (bit per player: 0=tech available, 1=tech disabled)")]];
    char rawcode[4] [[comment("Tech rawcode (can be: item, unit, or ability)")]];
};


struct Chance {
    u32 chance [[comment("Chance (percentage)")]];
    char rawcodes[Group.column_count*4] [[comment("Rawcodes")]];
};

struct Group {
    u32 num [[comment("Group number")]];
    char name[] [[comment("Group name")]];

    u32 column_count [[comment("Number of columns")]];
    u32 column_types[column_count] [[comment("Column types: 0=Units, 1=Buildings, 2=Items")]];

    u32 row_count [[comment("Number of rows")]];
    Chance chances[row_count];
};


struct ItemChance {
    u32 chance [[comment("Chance (percentage)")]];
    char rawcode[4] [[comment("Item rawcode")]];
};

struct ItemGroup {
    u32 item_count [[comment("Number of items in this set")]];
    ItemChance chances[item_count];
};

struct Item {
    u32 num [[comment("Table number")]];
    char name[] [[comment("Table name")]];
    u32 group_count [[comment("Number of item sets")]];
    ItemGroup groups[group_count];
};


struct Data {
    u32 save_count [[comment("Number of saves")]];
    u32 editor_version [[comment("World editor version")]];
    if (format > 28){
        u32 editor_version_full[4] [[comment("Full editor version (major.minor.build.revision)")]];
    }
    char map_name[] [[comment("Map name")]];
    char author[] [[comment("Map author")]];
    char description[] [[comment("Map description")]];
    char recommended_players[] [[comment("Recommended players description")]];
    Rect camera_bounds [[comment("Camera bounds rectangle")]];
    s32 non_playable_margins[4] [[comment("Non-playable area margins (A, B, C, D)")]];
    u32 playable_width [[comment("Playable area width (E); total width = A + E + B")]];
    u32 playable_height [[comment("Playable area height (F); total height = C + F + D")]];
    MapFlags map_flags;
    char tileset [[comment("Primary tileset")]];
    s32 loading_screen_preset [[comment("Campaign loading screen preset (-1 = none or custom imported)")]];
    if (format >= 25) char loading_screen_model[] [[comment("Custom loading screen model path")]];
    char loading_screen_text[] [[comment("Loading screen text")]];
    char loading_screen_title[] [[comment("Loading screen title")]];
    char loading_screen_subtitle[] [[comment("Loading screen subtitle")]];
    u32 game_data_set [[comment("Game data set (0 = default)")]];
    if (format >= 25) char prologue_screen_model[] [[comment("Prologue screen model path")]];
    char prologue_text[] [[comment("Prologue screen text")]];
    char prologue_title[] [[comment("Prologue screen title")]];
    char prologue_subtitle[] [[comment("Prologue screen subtitle")]];
    if (format >= 25) {
        u32 fog_type [[comment("Fog style (0 = none, >0 = fog style index)")]];
        float fog_z_start [[comment("Fog start Z-height")]];
        float fog_z_end [[comment("Fog end Z-height")]];
        float fog_density [[comment("Fog density")]];
        u32 fog_color [[comment("Fog color (RGBA)")]];
        char global_weather[4] [[comment("Global weather rawcode")]];
        char ambient_sound[] [[comment("Custom ambient sound environment")]];
        char tileset_light [[comment("Custom tileset lighting (e.g. 'A' = Ashenvale, 'L' = Lordaeron Summer)")]];
        u32 water_tint_color [[comment("Water tint color (RGBA)")]];
    }
    if (format >= 28) u32 script_language [[comment("Script language (0 = JASS, 1 = Lua)")]];
    if (format >= 31){
        u32;
        u32;
    }

    if (format >= 33){
        u32;
        u32;
        u32;
    }

    u32 player_count [[comment("Number of players")]];
    Player players[player_count];

    u32 force_count [[comment("Number of forces")]];
    Clan forces[force_count];

    u32 custom_upgrade_count [[comment("Number of custom upgrades")]];
    Upgrade custom_upgrades[custom_upgrade_count];

    u32 disabled_tech_count [[comment("Number of disabled techs")]];
    Tech disabled_techs[disabled_tech_count];

    u32 random_group_count [[comment("Number of random unit/building/item groups")]];
    Group random_groups[random_group_count];

    if (format >= 25){
        u32 random_item_table_count [[comment("Number of random item tables")]];
        Item random_item_tables[random_item_table_count];
    }
};


Data data @0x04;
