#pragma description Warcraft.doo
#pragma magic [ 57 33 64 6F ] @ 0x00
#pragma endian little
#pragma array_limit 99999

import std.io;
import std.mem;
import std.string;
import std.core;

char magick[4] @ 0x00 [[comment("W3do")]];
u32 format @0x04; // ROC=7, TFT=8
u32 subformat @0x08; // ROC=9/7, TFT=11

bool isUnit in;
u32 patch = 26; // Номер патча, указано в war3map.w3i

struct Vector {
    float x,y,z;
};

/*
При флагах 0 и 1 - разрушаемый объект больше не является разрушаемым (т.е его нельзя сломать и т.д)!
Если разрушаемый объект имеет выпадающие предметы, то он должен иметь флаг 0, а в скрипте карты на его месте должен появляться клон с выпадающими предметами
*/
enum Flag:u8 {
    invisible = 0, // Невидимый и проходимый (на нем можно строить и ходить через него)
    visible = 1, // Видимый и проходимый (становится полу-прозрачным, если за ним находится юнит, так же на нем можно строить и ходить через него)
    normal = 2,  // Обычный разрушаемый объект/обычная декорация
};

struct DropItem {
    char rawcode[4]; // Равкод предмета (0 = нету), Это может быть равкод случайного предмета (см. равкод случайного предмета)
    u32 chance; // Шанс выпадения в процентах
};


struct Drop {
    u32 count; // Кол-во предметов в наборе
    DropItem item[count];
};


struct Item {
    char rawcode[4];
    u32 variation; // если юнит можно игнорировать
    Vector position;
    float angle [[comment("radian")]];
    Vector scale; // если юнит можно игнорировать
    if (patch >= 32) u32 skin;
    Flag flag; // если юнит можно игнорировать

    if (isUnit){
        u32 player;
    } else {
        u8 health; // Здоровье patch >= 32 ? (от 0 до 255, 255 - это 100%) : (в процентах от 0% до 100%)
        if (format == 8){
            s32 dropIndex; // Номер таблицы случайных предметов (-1 = нету)
            u32 dropCount; // Количество наборов выпадаемых предметов (переменная j)
            Drop drop[dropCount];
        }
        u32 num; // 0 - Номер декорации
    }
};

struct Cliff {
    char rawcode[4];
    u32 variation; // Вариация
    u32 x; // .w3e координата
    u32 y; // .w3e координата
};

struct Data {
    u32 itemCount;

    Item item[itemCount];

    if (isUnit){}
    else {
        u32 cliffVersion; // Версия формата декораций склонов и рельефов
        u32 cliffCount; // Количество декораций склонов и рельефов
        Cliff cliff[cliffCount];
    }
};

Data data @ 0x0C;
