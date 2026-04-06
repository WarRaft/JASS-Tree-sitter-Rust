use serde::{Deserialize, Serialize};
use std::ops::Add;
use strum_macros::{Display, EnumIter};

#[derive(Debug, PartialEq, Serialize, Deserialize, EnumIter, Display, Clone, Copy)]
#[serde(rename_all = "camelCase")]
#[repr(u32)]
pub enum Kind {
    Namespace = 0, // For identifiers that declare or reference a namespace, module, or package.
    Class,         // For identifiers that declare or reference a class type.
    Enum,          // For identifiers that declare or reference an enumeration type.
    Interface,     // For identifiers that declare or reference an interface type.
    Struct,        // For identifiers that declare or reference a struct type.
    TypeParameter, // For identifiers that declare or reference a type parameter.
    Type,          // For identifiers that declare or reference a type that is not covered above.
    Parameter,     // For identifiers that declare or reference a function or method parameters.
    Variable,      // For identifiers that declare or reference a local or global variable.
    Property, // For identifiers that declare or reference a member property, member field, or member variable.
    EnumMember, // For identifiers that declare or reference an enumeration property, constant, or member.
    Decorator,  // For identifiers that declare or reference decorators and annotations.
    Event,      // For identifiers that declare an event property.
    Function,   // For identifiers that declare a function.
    Method,     // For identifiers that declare a member function or method.
    Macro,      // For identifiers that declare a macro.
    Label,      // For identifiers that declare a label.
    Comment,    // For tokens that represent a comment.
    String,     // For tokens that represent a string literal.
    Keyword,    // For tokens that represent a language keyword.
    Number,     // For tokens that represent a number literal.
    Regexp,     // For tokens that represent a regular expression literal.
    Operator,   // For tokens that represent an operator.
}

#[repr(u32)]
#[derive(Debug, Serialize, Deserialize, EnumIter, Display, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Mod {
    Declaration = 0,
    Definition,
    Readonly,
    Static,
    Deprecated,
    Abstract,
    Async,
    Modification,
    Documentation,
    DefaultLibrary,
}

impl From<Mod> for u32 {
    fn from(m: Mod) -> u32 {
        1 << (m as u32)
    }
}

impl Add for Mod {
    type Output = u32;

    fn add(self, rhs: Mod) -> u32 {
        (1 << self as u32) | (1 << rhs as u32)
    }
}

impl Add<Mod> for u32 {
    type Output = u32;

    fn add(self, rhs: Mod) -> u32 {
        self | (1 << rhs as u32)
    }
}
