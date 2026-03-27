#pragma description Warcraft.w3e
#pragma endian little
#pragma array_limit 999999

import std.io;
import std.mem;
import std.string;
import std.core;

struct Rawcode {
    char value [4];
} [[comment(value)]];

enum Land : char {
  Ashenvale        = 'A', // Ashenvale
  Barrens          = 'B', // Barrens
  BlackCitadel     = 'K', // Black Citadel
  Cityscape        = 'Y', // Cityscape
  Dalaran          = 'X', // Dalaran
  DalaranRuins     = 'J', // Dalaran Ruins
  Dungeon          = 'D', // Dungeon
  Felwood          = 'C', // Felwood
  IcecrownGlacier  = 'I', // Icecrown Glacier
  LordaeronFall    = 'F', // Lordaeron Fall
  LordaeronSummer  = 'L', // Lordaeron Summer
  LordaeronWinter  = 'W', // Lordaeron Winter
  Northrend        = 'N', // Northrend
  Outland          = 'O', // Outland
  SunkenRuins      = 'Z', // Sunken Ruins
  Underground      = 'G', // Underground
  Village          = 'V', // Village
  VillageFall      = 'Q'  // Village Fall
};

struct Point {
    u16 a; //short   Высота земли 8192 (0x2000) = нулевая высота
    u16 b; //short   Высота воды и флаг границы обзора   Высота воды = первые 14 бит (начиная с младшего), флаг = 0x4000. Код: water_height=val&0x3FFF; edge_flag=val&0x4000
    u8 c;//byte    Текстура земли и флаги   Текстура земли = номер текстуры в списке тайлсетов, флаги = см. флаги. Код: texture=val&0x0F
    u8 d; //byte    Вариация земли и склона Код: ground_variation=val&0x1F; cliff_variation=(val&0xE0)>>5
    u8 e; //byte    Высота слоя и текстура склона  Высота слоя меняется командами: Увеличить на 1/Уменьшить на 1/и т.д. Код: layer_height=val&0x0F; cliff_texture=(val&0xF0)>>4
};


struct Data {
    Rawcode magick [[comment("Магическое число 'W3E!'")]];
    s32 format; //  Версия формата 11
    Land land; // Базовый тип ландшафта    см. типы ландшафта
    s32 customLand; // Нестандартный тип ландшафта    0 = не используется, 1 = используется
    s32 tileCount; //Количество используемых тайлсетов    Переменная i
    Rawcode tiles [tileCount]; //  Равкоды используемых тайлсетов  Список см. в "TerrainArt\Terrain.slk"
    s32 cliffCount; // Количество используемых тайлсетов склонов Переменная i
    Rawcode cliffs [cliffCount]; //   Равкоды используемых тайлсетов склонов   Список см. в "TerrainArt\CliffTypes.slk"
    s32 width; // Ширина карты +1  Переменная x
    s32 height; //Высота карты +1  Переменная y
    float left; // Смещение влево от нулевых координат (0.00,0.00) по оси X    Переменная left. В редакторе это "граница карты влево", "граница карты вправо" вычисляется по формуле (x-1)*128+left
    float bottom; // Смещение вниз от нулевых координат (0.00,0.00) по оси Y  Переменная bottom. В редакторе это "граница карты вниз", "граница карты вверх" вычисляется по формуле (y-1)*128+bottom
    Point points [height * width];
};

Data data @0x0;
