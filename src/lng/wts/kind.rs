use num_enum::{IntoPrimitive, TryFromPrimitive};

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromPrimitive, IntoPrimitive)]
pub enum Kind {
    StringKeyword = 1,
    Identifier = 2,
    Comment = 3,
    LeftBrace = 4,
    RightBrace = 5,
    StringText = 6,
    // _space = 7,
    // _newline = 8,
    SourceFile = 9,
    Header = 10,
    StringLiteral = 11,
    StringLine = 12,
}

