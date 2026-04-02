use num_enum::{IntoPrimitive, TryFromPrimitive};

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromPrimitive, IntoPrimitive)]
pub enum Kind {
    RightBracket = 1,
    Equal = 2,
    DoubleQuote = 3,
    LineBreak = 4,
    LBracket = 5,
    Int = 6,
    Float = 7,
    StringContent = 8,
    UnquotedString = 9,
    Whitespace = 10,
    Comma = 11,
    LineComment = 12,
    SectionName = 13,
    Key = 14,
    Program = 15,
    //_Line = 16,
    //_LineContent = 17,
    Section = 18,
    Comment = 19,
    Item = 20,
    QuotedString = 21,
    ValueList = 22,
    // aux_sym... начинаются с 23+
}
