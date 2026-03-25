use num_enum::{IntoPrimitive, TryFromPrimitive};

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, TryFromPrimitive, IntoPrimitive)]
pub enum Kind {
    // sym__newline = 1,
    IdKeyword = 2,       // anon_sym_ID
    Semicolon = 3,       // anon_sym_SEMI
    EndRecord = 4,       // sym_end_record  (was anon_sym_E + sym_end_record)
    TailContent = 5,     // sym_tail_content
    BKeyword = 6,        // anon_sym_B
    CKeyword = 7,        // anon_sym_C
    FKeyword = 8,        // anon_sym_F
    OKeyword = 9,        // anon_sym_O
    PKeyword = 10,       // anon_sym_P
    // aux_sym_format_field_token1 = 11,
    FieldTag = 12,       // sym_field_tag
    FieldValue = 13,     // sym_field_value
    SourceFile = 14,     // sym_source_file
    IdRecord = 15,       // sym_id_record
    Tail = 16,           // sym_tail
    // _record = 17,
    BRecord = 18,        // sym_b_record
    CRecord = 19,        // sym_c_record
    FRecord = 20,        // sym_f_record
    ORecord = 21,        // sym_o_record
    PRecord = 22,        // sym_p_record
    FormatField = 23,    // sym_format_field
    Field = 24,          // sym_field
}

