use num_enum::{IntoPrimitive, TryFromPrimitive};

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromPrimitive, IntoPrimitive)]
pub enum Kind {
    // sym__newline = 1,
    IdKeyword = 2,    // anon_sym_ID
    Semicolon = 3,    // anon_sym_SEMI
    EKeyword = 4,     // anon_sym_E
    BKeyword = 5,     // anon_sym_B
    CKeyword = 6,     // anon_sym_C
    FKeyword = 7,     // anon_sym_F
    OKeyword = 8,     // anon_sym_O
    PKeyword = 9,     // anon_sym_P
    // aux_sym_format_field_token1 = 10,
    FieldTag = 11,    // sym_field_tag
    FieldValue = 12,  // sym_field_value
    SourceFile = 13,  // sym_source_file
    IdRecord = 14,    // sym_id_record
    EndRecord = 15,   // sym_end_record
    // _record = 16,
    BRecord = 17,     // sym_b_record
    CRecord = 18,     // sym_c_record
    FRecord = 19,     // sym_f_record
    ORecord = 20,     // sym_o_record
    PRecord = 21,     // sym_p_record
    FormatField = 22, // sym_format_field
    Field = 23,       // sym_field
}

