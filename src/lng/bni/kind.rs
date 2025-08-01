use num_enum::{IntoPrimitive, TryFromPrimitive};

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromPrimitive, IntoPrimitive)]
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
    //_Line = 15,
    //_LineContent = 16,
    Section = 17,
    Comment = 18,
    Item = 19,
    ValueList = 20,
    // aux_sym... начинаются с 21+
}
