#pragma description Warcraft.w3i
#pragma endian little
#pragma array_limit 999999

import std.io;
import std.mem;
import std.string;
import std.core;

u32 format @ 0x00; // ROC=18, TFT=25, 1.31=28, 1.32=31, 2.0.3 = 33

struct Color {
    u8 b [[color("0000FF")]];
    u8 g [[color("00FF00")]];
    u8 r [[color("FF0000")]];
    u8 a [[color("000000")]];
} [[static, color(std::format("{:02X}{:02X}{:02X}", r, g, b))]];;

bitfield MapFlags {
    hide_minimap             : 1 [[comment("Скрыть мини-карту на экранах предпросмотра")]];
    ally_priorities_edit     : 1 [[comment("Изменить приоритеты союзников")]];
    melee_map                : 1 [[comment("Карта подходит для режима \"Сражения\"")]];
    custom_terrain_type      : 1 [[comment("Использовать нестандартный тип ландшафта")]];
    partial_visibility       : 1 [[comment("Скрытые области частично видимы")]];
    fixed_player_settings    : 1 [[comment("Фикс. параметры игрока")]];
    custom_forces            : 1 [[comment("Нестандартные кланы")]];
    custom_techs             : 1 [[comment("Нестандартные технологии")]];
    custom_abilities         : 1 [[comment("Нестандартные способности")]];
    custom_upgrades          : 1 [[comment("Нестандартные улучшения")]];
    unknown_flag_0x400       : 1 [[comment("Неизвестно")]];
    waves_steep_shore        : 1 [[comment("Показывать волны на крутых берегах")]];
    waves_shallow_shore      : 1 [[comment("Показывать волны на пологих берегах")]];
    terrain_fog              : 1 [[comment("Используется туман на местности")]];
    expansion_required       : 1 [[comment("Требуется дополнение")]];
    item_classification      : 1 [[comment("Классификация предметов")]];
    water_color_override     : 1 [[comment("Фоновый цвет воды")]];
    padding                  : 15;
};

enum PlayerType: u32 {
    Human    = 1, // Человек
    Comp     = 2, // Компьютер
    Neutral  = 3, // Нейтрал
    Reserve  = 4  // Резерв
};

enum Race: u32 {
    Random   = 0, // Случайная раса
    Human    = 1, // Альянс
    Orc      = 2, // Орда
    Undead   = 3, // Нежить
    NightElf = 4  // Ночные эльфы
};

bitfield PlayerBool {
    Player1   : 1 [[comment("Игрок 1")]];
    Player2   : 1 [[comment("Игрок 2")]];
    Player3   : 1 [[comment("Игрок 3")]];
    Player4   : 1 [[comment("Игрок 4")]];
    Player5   : 1 [[comment("Игрок 5")]];
    Player6   : 1 [[comment("Игрок 6")]];
    Player7   : 1 [[comment("Игрок 7")]];
    Player8   : 1 [[comment("Игрок 8")]];
    Player9   : 1 [[comment("Игрок 9")]];
    Player10  : 1 [[comment("Игрок 10")]];
    Player11  : 1 [[comment("Игрок 11")]];
    Player12  : 1 [[comment("Игрок 12")]];
    Player13  : 1 [[comment("Игрок 13")]];
    Player14  : 1 [[comment("Игрок 14")]];
    Player15  : 1 [[comment("Игрок 15")]];
    Player16  : 1 [[comment("Игрок 16")]];
    Player17  : 1 [[comment("Игрок 17")]];
    Player18  : 1 [[comment("Игрок 18")]];
    Player19  : 1 [[comment("Игрок 19")]];
    Player20  : 1 [[comment("Игрок 20")]];
    Player21  : 1 [[comment("Игрок 21")]];
    Player22  : 1 [[comment("Игрок 22")]];
    Player23  : 1 [[comment("Игрок 23")]];
    Player24  : 1 [[comment("Игрок 24")]];
    Player25  : 1 [[comment("Игрок 25")]];
    Player26  : 1 [[comment("Игрок 26")]];
    Player27  : 1 [[comment("Игрок 27")]];
    Player28  : 1 [[comment("Игрок 28")]];
    Player29  : 1 [[comment("Игрок 29")]];
    Player30  : 1 [[comment("Игрок 30")]];
    Player31  : 1 [[comment("Игрок 31")]];
    Player32  : 1 [[comment("Игрок 32")]];
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
    u32 num; // Номер игрока
    PlayerType type;
    Race race;
    u32 fix; // Фиксированная исходная позиция  Самый младший бит (0=нет, 1=да)
    char name[]; // Имя игрока
    Point pos; // Координата стартовой позиции
    PlayerBool p_down; // Низкие приоритеты союзников    От младшего бита к старшему (0=приоритета на игрока нету, 1=приоритет на игрока есть)
    PlayerBool p_up; // Высокие приоритеты союзников  От младшего бита к старшему (0=приоритета на игрока нету, 1=приоритет на игрока есть)
    if (format >= 31){
        u32;
        u32;
    }
};

bitfield ClanFlags {
    allied                : 1 [[comment("Союзник")]];
    shared_victory        : 1 [[comment("Общая победа")]];
    unknown_flag_0x0004   : 1 [[comment("Неизвестно")]];
    shared_vision         : 1 [[comment("Общее поле зрения")]];
    shared_units          : 1 [[comment("Общие войска")]];
    shared_all_units      : 1 [[comment("Общие войска:все")]];
    padding               : 26;
};

struct Clan {
    ClanFlags flags;
    u32 players; // От младшего бита к старшему (0=игрока нету в клане, 1=игрок есть в клане), но надо учитывать наличие самого игрока в игре, т.к в первом клане по умолчанию присутствуют все игроки (от 0 до 31), которых нету в других кланах
    char name[]; // Название клана
};


enum UpgradeStatus: u32 {
    Disabled   = 0, // Недоступно
    Available  = 1, // Доступно
    Researched = 2  // Исследовано
};

struct Upgrade {
    PlayerBool players; // Игроки    От младшего бита к старшему (0=на игрока не действует улучшение, 1=на игрока действует улучшение)
    char id[4]; // Равкод улучшения
    u32 level; //int Уровень улучшения
    UpgradeStatus status; // Доступность улучшения
};

struct Tech {
    PlayerBool players; //int Игроки    От младшего бита к старшему (0=игроку ДОСТУПНА технология, 1=игроку НЕ ДОСТУПНА технология)
    char id[4]; // Равкод технологии   Это может быть: предмет, юнит, способность
};


struct Chance {
    u32 chance; //int Шанс (в процентах)
    char ids[Group.j*4]; //   Равкоды
};

struct Group {
    u32 num; //int Номер группы
    char name []; //string  Название группы

    u32 j; //int Кол-во позиций (колонн)   Переменная j
    u32 j_type[j]; // Тип позиций   0=Войска, 1=Здания, 2=Предметы

    u32 k; // int Кол-во наборов (рядов) Переменная k
    Chance chances[k];
};


struct ItemChance {
    u32 chance; //int Шанс (в процентах)
    char ids[4]; // Равкод предмета
};

struct ItemGroup {
    u32 k; //int Кол-во предметов в наборе  Переменная k
    ItemChance chances[k];
};

struct Item {
    u32 num; // int Номер таблицы
    char name[]; // string  Название таблицы
    u32 j; // int Кол-во наборов предметов   Переменная j
    ItemGroup groups[j];
};


struct Data {
    u32 save_count;
    u32 editor_version;
    if (format > 28){
        u32 editor_version_full[4];
    }
    char map_name[];
    char author[];
    char description[];
    char players_description[];
    Rect cam_bounds;
    s32 map_size[4]; // Размеры неигровой области карты Переменные A, B, C, D см. ниже
    u32 map_width; //Ширина игровой области карты   E, ширина карты = A+E+B
    u32 map_height; // Высота игровой области карты   F, высота карты = C+F+D
    MapFlags map_flags;
    char land; // Основной тип ландшафта карты
    s32 loadscreen_num; // Номер загрузочного экрана от стандартных кампаний -1 = нету или используется импортированный загрузочный экран
    if (format >= 25) char loadscreen_path[]; // Путь импортированного загрузочного экрана
    char loadscreen_text[]; // Текст загрузочного экрана
    char loadscreen_title[]; // Заголовок загрузочного экрана
    char loadscreen_subtitle[]; // Подзаголовок загрузочного экрана
    u32 game_data_set; // Используемый набор игровых данных Номер предустановки, 0=Стандартный
    if (format >= 25) char prologue_path[]; // Путь импортированного экрана пролога
    char prologue_text[]; // Текст экрана пролога
    char prologue_title[]; // Заголовок экрана пролога
    char prologue_subtitle[]; // Подзаголовок экрана пролога
    if (format >= 25) {
        u32 fog; // 0=Не используется, больше нуля = номер в списке стилей тумана
        float fog_start; // Начальная Z-высота тумана
        float fog_end; // Конечная Z-высота тумана
        float fog_density; // Плотность тумана
        u32 fog_color; // Цвет тумана RGBA
        char weather[4]; // Равкод глобальной погоды на карте
        char sound[]; // Нест. звуковое окружение
        char light; // Нестандартное освещение Пример: 'A' - Ашенваль, 'L' - Летний Лордерон
        u32 water_color; // Фоновый цвет воды RGBA
    }
    if (format >= 28) u32 is_lua; // 0=JASS, 1=Lua
    if (format >= 31){
        u32;
        u32;
    }

    if (format >= 33){
        u32;
        u32;
        u32;
    }

    u32 player_count; // Кол-во игроков
    Player players[player_count];

    u32 clan_count; // Кол-во кланов
    Clan clans[clan_count];

    u32 upgrade_count; // Кол-во нестандартных улучшений
    Upgrade upgrades[upgrade_count];

    u32 tech_count; // Кол-во нестандартных технологий
    Tech techs[tech_count];

    u32 group_count; // Кол-во случайных групп
    Group groups[group_count];

    if (format >= 25){
        u32 item_count; // Кол-во таблиц предметов
        Item items[item_count];
    }
};


Data data @0x04;
