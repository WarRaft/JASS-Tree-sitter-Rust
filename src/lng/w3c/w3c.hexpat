#pragma description Warcraft III Camera File (.w3c)
#pragma endian little
#pragma array_limit 999999

// ── war3map.w3c ─────────────────────────────────────────────────────
// Reference: https://xgm.guru/p/wc3/w3-file-format

import std.io;
import std.mem;
import std.string;
import std.core;

// ── Header ──────────────────────────────────────────────────────────

u32 format @ 0x00 [[comment("Format version")]];
u32 count  @ 0x04 [[comment("Number of cameras")]];

// ── Types ───────────────────────────────────────────────────────────

struct Color {
    u8 b [[color("0000FF")]];
    u8 g [[color("00FF00")]];
    u8 r [[color("FF0000")]];
    u8 a [[color("000000")]];
} [[static, color(std::format("{:02X}{:02X}{:02X}", r, g, b))]];

struct Camera {
float	a1; [[comment("X")]];
float	a2;// Y
float	a3;// Z
float	a4;// Поворот (в градусах)
float	a5;// Угол атаки (в градусах)
float	a6;// Расстояние
float	a7;// Крен
float	a8;// Поле зрения (в градусах)
float	a9;// Дальняя граница
float	a10;// Ближняя граница
char  name[]
};

// ── Data ────────────────────────────────────────────────────────────

Camera cameras[count] @ 0x08;
