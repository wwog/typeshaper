extern crate self as typeshaper;

mod error;
mod transform;

pub use error::RequiredError;
pub use transform::{TypeshaperExt, TypeshaperInto};
pub use typeshaper_macros::{typeshaper, typex, __typeshaper_import};

pub mod prelude {
    pub use typeshaper_macros::{typeshaper, typex, __typeshaper_import};
    pub use crate::error::RequiredError;
    pub use crate::transform::{TypeshaperExt, TypeshaperInto};
}
