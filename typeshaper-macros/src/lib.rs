mod expand;
mod parse;
mod state;

use proc_macro::TokenStream;
use syn::{DeriveInput, Ident, parse_macro_input};

/// Registers a struct's fields in the global type registry.
///
/// The struct itself is re-emitted unchanged. This attribute is the only
/// prerequisite before using `typex!()` on a type.
///
/// ## Variants
///
/// - `#[typeshaper]` — local registration only.
/// - `#[typeshaper(export)]` — local registration **plus** generation of a
///   companion `typeshaper_import_TypeName!()` macro that other crates can call
///   to register the same type in their own compilation unit, enabling
///   cross-crate `typex!()` operations.
///
/// # Example
/// ```ignore
/// #[typeshaper]
/// #[derive(Debug, Clone)]
/// pub struct User {
///     pub id: u64,
///     pub name: String,
///     pub age: u8,
/// }
///
/// // In another crate:
/// #[typeshaper(export)]
/// pub struct Product { pub id: u64, pub title: String }
/// // → generates pub macro typeshaper_import_Product!()
/// ```
#[proc_macro_attribute]
pub fn typeshaper(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as DeriveInput);
    if attr.is_empty() {
        return expand::register_typeshaper(input).into();
    }
    match syn::parse::<Ident>(attr) {
        Ok(ident) if ident == "export" => expand::register_typeshaper_export(input).into(),
        Ok(ident) => syn::Error::new(
            ident.span(),
            format!(
                "unknown typeshaper option `{ident}`; \
                 expected `#[typeshaper]` or `#[typeshaper(export)]`"
            ),
        )
        .to_compile_error()
        .into(),
        Err(_) => syn::Error::new(
            proc_macro2::Span::call_site(),
            "invalid typeshaper attribute; expected `#[typeshaper]` or `#[typeshaper(export)]`",
        )
        .to_compile_error()
        .into(),
    }
}

/// Internal proc-macro called by the generated `typeshaper_import_TypeName!()`
/// companion macro. Parses inline field tokens and registers the type in the
/// current crate's compilation-time HashMap. Emits no code.
///
/// Format: `TypeName, [vis] field1: Type1, [vis] field2: Type2 [InnerType2], ...`
///
/// The optional visibility token(s) before a field name (`pub`, `pub(crate)`, …)
/// preserve the original field visibility across the crate boundary.
///
/// The optional `[InnerType]` after a field's type carries the pre-`Option`
/// inner type recorded by a `Partial` (`T?`) operation, so that `Required`
/// (`T!`) works correctly in the consuming crate.
///
/// Not intended for direct use. Call `typeshaper_import_TypeName!()` instead.
#[doc(hidden)]
#[proc_macro]
pub fn __typeshaper_import(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as parse::ImportInput);
    expand::expand_import(input).into()
}

/// Unified type-algebra macro. Creates a new named struct type by applying
/// one operator to one or two registered types.
///
/// | Syntax              | Operation | Generated impls                    |
/// |---------------------|-----------|------------------------------------|
/// | `T`                 | Rebuild   | `TypeshaperInto<Target> for T`     |
/// | `T - [f1, f2]`      | Omit      | `TypeshaperInto<Target> for T`     |
/// | `T & [f1, f2]`      | Pick      | `TypeshaperInto<Target> for T`     |
/// | `A + B`             | Merge     | `From<(A, B)> for Target`          |
/// | `T?`                | Partial   | `From<T> for Target`               |
/// | `T!`                | Required  | `TryFrom<T> for Target`（源无 Option 字段时为 `From<T>`）|
/// | `A % B`             | Diff      | `TypeshaperInto<Target> for A`     |
///
/// Every generated type is itself registered so it can be used as a source
/// in subsequent `typex!()` calls.
///
/// # Examples
/// ```ignore
/// typex!(#[derive(Serialize)] UserDto = User);  // Rebuild — same fields, new derives
/// typex!(NoAge    = User - [age]);               // Omit
/// typex!(Public   = User & [id, name]);          // Pick
/// typex!(Full     = NoAge + Badge);              // Merge
/// typex!(Draft    = User?);                      // Partial
/// typex!(Complete = Draft!);                     // Required
/// typex!(Diff     = User % Badge);               // Diff
/// ```
#[proc_macro]
pub fn typex(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as parse::ShapeInput);
    expand::expand_shape(input).into()
}
