#pragma description Warcraft.w3e
#pragma endian little
#pragma array_limit 999999

import std.io;
import std.mem;
import std.string;
import std.core;

struct Rawcode {
    char value[4];
} [[comment(value)]];

enum Tileset : char {
    Ashenvale       = 'A',
    Barrens         = 'B',
    BlackCitadel    = 'K',
    Cityscape       = 'Y',
    Dalaran         = 'X',
    DalaranRuins    = 'J',
    Dungeon         = 'D',
    Felwood         = 'C',
    IcecrownGlacier = 'I',
    LordaeronFall   = 'F',
    LordaeronSummer = 'L',
    LordaeronWinter = 'W',
    Northrend       = 'N',
    Outland         = 'O',
    SunkenRuins     = 'Z',
    Underground     = 'G',
    Village         = 'V',
    VillageFall     = 'Q'
};

bitfield WaterAndEdge {
    waterHeight : 14 [[comment("Water height (val & 0x3FFF)")]];
    edgeFlag    : 1  [[comment("Camera boundary flag (val & 0x4000)")]];
    padding     : 1;
};

bitfield TextureAndFlags {
    groundTexture : 4 [[comment("Ground texture index in tileset list (val & 0x0F)")]];
    ramp          : 1 [[comment("Ramp flag: allows units to walk between layers")]];
    blight        : 1 [[comment("Blight flag")]];
    water         : 1 [[comment("Water flag")]];
    boundary      : 1 [[comment("Boundary flag")]];
};

bitfield VariationData {
    groundVariation : 5 [[comment("Ground texture variation (val & 0x1F)")]];
    cliffVariation  : 3 [[comment("Cliff variation ((val & 0xE0) >> 5)")]];
};

bitfield LayerData {
    layerHeight  : 4 [[comment("Layer height (val & 0x0F)")]];
    cliffTexture : 4 [[comment("Cliff texture ((val & 0xF0) >> 4)")]];
};

struct Point {
    u16             groundHeight [[comment("Ground height. 8192 (0x2000) = zero height")]];
    WaterAndEdge    waterAndEdge [[comment("Water height + camera boundary flag")]];
    TextureAndFlags textureFlags [[comment("Ground texture + point flags")]];
    VariationData   variation    [[comment("Ground and cliff variation")]];
    LayerData       layer        [[comment("Layer height and cliff texture")]];
};

struct Data {
    Rawcode magic           [[comment("Magic number 'W3E!'")]];
    s32     version         [[comment("Format version (11)")]];
    Tileset tileset         [[comment("Base tileset type")]];
    s32     customTileset   [[comment("Custom tileset (0 = no, 1 = yes)")]];
    s32     groundTileCount [[comment("Number of ground tilesets used")]];
    Rawcode groundTiles[groundTileCount] [[comment("Ground tileset rawcodes (TerrainArt\\Terrain.slk)")]];
    s32     cliffTileCount  [[comment("Number of cliff tilesets used")]];
    Rawcode cliffTiles[cliffTileCount]   [[comment("Cliff tileset rawcodes (TerrainArt\\CliffTypes.slk)")]];
    s32     mapWidth        [[comment("Map width + 1")]];
    s32     mapHeight       [[comment("Map height + 1")]];
    float   offsetX         [[comment("X offset from origin. Right boundary = (mapWidth-1)*128 + offsetX")]];
    float   offsetY         [[comment("Y offset from origin. Top boundary = (mapHeight-1)*128 + offsetY")]];
    Point   points[mapHeight * mapWidth];
};

Data data @0x0;
