#pragma description Warcraft.w3abdhqtu
#pragma endian little
#pragma array_limit 99999

import std.io;
import std.mem;
import std.string;
import std.core;

bool leveldata in; // .w3a, .w3d, .w3q

/*
war3map.w3a	Ability data
war3map.w3b	Destructable data
war3map.w3d	Doodad data
war3map.w3h	Buff data
war3map.w3q	Upgrade data
war3map.w3t	Item data
war3map.w3u	Unit data (in W3M maps only .w3u is used, storing both unit and item data)
*/

u32 formatVersion @ 0x00;

struct Modification {
    char modificationId[4];
    u32 valueType;

    if (leveldata) {
        u32 level;
        u32 dataIndex;
    }

    match (valueType) {
        (0): u32 intValue;
        (1): float realValue;
        (2): float unrealValue; // 0.0 - 1.0
        (3): char stringValue[];
    }

    u32 terminator;
};

struct ModificationSet {
    if (formatVersion >= 3) u32 flags;
    u32 modificationCount;
    Modification modifications[modificationCount];
};

struct ObjectDefinition {
    char originalId[4];
    char customId[4];

    if (formatVersion >= 3) {
        u32 setCount;
        ModificationSet sets[setCount];
    } else {
        ModificationSet sets;
    }
};

struct ObjectTable {
    u32 originalCount;
    ObjectDefinition originals[originalCount];

    u32 customCount;
    ObjectDefinition customs[customCount];
};

ObjectTable objectTable @ 0x04;
