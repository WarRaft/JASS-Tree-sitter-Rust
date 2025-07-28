use strum_macros::{AsRefStr, Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, Display, AsRefStr)]
#[strum(serialize_all = "snake_case")]
pub enum Kind {
    Blank,
    Item,
    Int,
    Program,
    Section,
    ValueList,
    Comma,
    Comment,
    Key,
    LineBreak,
    QuotedString,
    SectionName,
    UnquotedString,
    Whitespace,

    #[strum(serialize = "=")]
    Equal,

    #[strum(serialize = "[")]
    LeftBracket,

    #[strum(serialize = "]")]
    RightBracket,

    #[strum(serialize = "ERROR")]
    Error,
}
