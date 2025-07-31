use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromPrimitive, IntoPrimitive)]
#[repr(u16)]
pub enum Kind {
    RightBracket = 1,
    Equal = 2,
    LineBreak = 3,
    LeftBracket = 4,
    Int = 5,
    Float = 6,
    QuotedString = 7,
    UnquotedString = 8,
    Whitespace = 9,
    Comma = 10,
    LineComment = 11,
    SectionName = 12,
    Key = 13,
    Program = 14,
    // _line           = 15, // internal, пропускаем
    Section = 16,
    Comment = 17,
    Item = 18,
    ValueList = 19,
    // aux ...         = 20+
}
