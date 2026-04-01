use num_enum::{IntoPrimitive, TryFromPrimitive};

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromPrimitive, IntoPrimitive)]
pub enum Kind {
    RightBracket = 1,
    Equal = 2,
    DoubleQuote = 3,
    SingleQuote = 4,
    LineBreak = 5,
    LBracket = 6,
    Int = 7,
    Float = 8,
    DqStringContent = 9,
    SqStringContent = 10,
    UnquotedString = 11,
    Whitespace = 12,
    Comma = 13,
    LineComment = 14,
    SectionName = 15,
    Key = 16,
    Program = 17,
    //_Line = 18,
    //_LineContent = 19,
    Section = 20,
    Comment = 21,
    Item = 22,
    QuotedString = 23,
    ValueList = 24,
    // aux_sym... начинаются с 25+
}
