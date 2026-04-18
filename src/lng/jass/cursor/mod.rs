
// ─── Submodules ──────────────────────────────────────────────────────────────
pub mod annotations;
pub mod helpers;
pub mod imports;
pub mod import_linker;
pub mod leak_check;
pub mod diagnostics;
pub mod ref_tracking;
pub mod semantic_tokens;
pub mod state;
pub mod stmt_visitor;
pub mod type_system;
pub mod walker;
pub(super) use ref_tracking::{HlScope, UnresolvedRef};
pub(super) use annotations::extract_annotations;
pub use imports::{ImportedKind, ImportedSymbol};
pub use state::{Cursor, Scope, VarInfo};

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod state_test;
#[cfg(test)]
mod semantic_tokens_test;
#[cfg(test)]
mod ref_tracking_test;
#[cfg(test)]
mod type_system_test;
#[cfg(test)]
mod leak_check_test;
#[cfg(test)]
mod annotations_test;

