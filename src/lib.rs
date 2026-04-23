extern crate self as typeshaper;

mod error;
mod transform;

pub use error::RequiredError;
pub use transform::{TypeshaperExt, TypeshaperInto};
pub use typeshaper_macros::{typeshaper, typex, __typeshaper_import};

/// Wire-format version embedded in `#[typeshaper(export)]`-generated companion macros.
/// Consumer crates verify this constant matches at compile time to detect version mismatches.
#[doc(hidden)]
pub const TYPESHAPER_WIRE_FORMAT_VERSION: u32 = 2;

pub mod prelude {
    pub use typeshaper_macros::{typeshaper, typex};
    pub use crate::error::RequiredError;
    pub use crate::transform::{TypeshaperExt, TypeshaperInto};
}
