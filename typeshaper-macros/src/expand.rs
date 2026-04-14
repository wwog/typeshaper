use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Fields, Ident};

use crate::parse::{ImportInput, ShapeExpr, ShapeInput, ShapeNode};
use crate::state::{FieldDef, lookup, lookup_exported, next_anon_id, register, register_exported, register_import};

// ---------------------------------------------------------------------------
// Crate-scoped registry key
// ---------------------------------------------------------------------------

/// Return the `CARGO_MANIFEST_DIR` of the crate currently being compiled.
///
/// Cargo sets this env-var to a unique directory per crate before invoking
/// rustc.  rust-analyzer also propagates it through its proc-macro server
/// protocol, so different crates in the same workspace receive different
/// values even though they share one server process.
///
/// Using this as a registry-key prefix means identically-named types from
/// different crates never collide, eliminating spurious IDE errors while
/// keeping per-crate isolation correct for `cargo build` / `cargo test`.
///
/// Falls back to `None` in unit-test contexts or other environments where
/// the variable is absent; in that case all entries share a common "global"
/// namespace, which is harmless for isolated compilations.
fn crate_key() -> Option<String> {
    std::env::var("CARGO_MANIFEST_DIR").ok()
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Dispatch a `typex!( [#[attr...]] Target = Expr )` invocation.
///
/// Intermediate anonymous types produced by sub-expressions are emitted before
/// the final target type, all in a single `TokenStream`.
pub fn expand_shape(input: ShapeInput) -> TokenStream {
    let attrs = input.attrs;
    let target = input.target;
    let hint = target.to_string();
    let mut acc: Vec<TokenStream> = Vec::new();

    let result = expand_expr(input.expr, &attrs, target, &hint, &mut acc);

    match result {
        Ok(main_ts) => {
            let mut output = TokenStream::new();
            for ts in acc {
                output.extend(ts);
            }
            output.extend(main_ts);
            output
        }
        Err(ts) => ts,
    }
}

/// Register a `#[typeshaper]` struct in the global registry and re-emit it.
///
/// No conflict detection is performed here. A proc-macro's global registry is
/// shared across all crates in a workspace when tools like rust-analyzer reuse
/// the same proc-macro server process. Emitting an error for a "conflict" that
/// is merely a different crate's identically-named type would produce spurious
/// IDE errors. Same-name conflicts within a single crate are caught by Rust's
/// own type system at the call sites of `typex!()`.
pub fn register_typeshaper(input: DeriveInput) -> TokenStream {
    let file      = crate_key();
    let type_name = input.ident.to_string();

    let fields = match extract_named_fields(&input) {
        Ok(f)  => f,
        Err(e) => return e,
    };

    register(file, type_name, fields);
    quote! { #input }
}

/// Register a `#[typeshaper(export)]` struct and additionally generate the
/// companion `typeshaper_import_TypeName!()` macro that encodes field metadata
/// as tokens for use in other crates.
pub fn register_typeshaper_export(input: DeriveInput) -> TokenStream {
    match try_register_typeshaper_export(input) {
        Ok(ts) => ts,
        Err(ts) => ts,
    }
}

fn try_register_typeshaper_export(input: DeriveInput) -> R {
    let type_ident = &input.ident;
    let file       = crate_key();
    let type_name  = type_ident.to_string();

    let fields = extract_named_fields(&input)?;

    register(file, type_name.clone(), fields.clone());
    // Also write to the export registry so that consuming crates can find this
    // type even before `typeshaper_import_TypeName!()` has been expanded in their
    // context (handles rust-analyzer's non-deterministic macro expansion order).
    register_exported(type_name, fields.clone());

    let macro_name = format!("typeshaper_import_{}", type_ident);
    let macro_ident: Ident = syn::parse_str(&macro_name).map_err(|e| {
        syn::Error::new(type_ident.span(), format!("typeshaper: cannot form macro name `{}`: {}", macro_name, e))
            .to_compile_error()
    })?;

    let field_entries: Vec<TokenStream> = fields
        .iter()
        .map(|f| -> R {
            let fname: Ident = syn::parse_str(&f.name).map_err(|e| {
                syn::Error::new(proc_macro2::Span::call_site(), format!("typeshaper: invalid field name `{}`: {}", f.name, e))
                    .to_compile_error()
            })?;
            let ftype: TokenStream = f.ty_tokens.parse().map_err(|e| {
                syn::Error::new(proc_macro2::Span::call_site(), format!("typeshaper: invalid type tokens `{}`: {}", f.ty_tokens, e))
                    .to_compile_error()
            })?;
            // Encode visibility so the consuming crate can restore it faithfully.
            // Visibility::Inherited produces an empty TokenStream, which is correct
            // for private fields (no keyword in the encoded format).
            let fvis: TokenStream = f.vis.parse().map_err(|e| {
                syn::Error::new(proc_macro2::Span::call_site(), format!("typeshaper: invalid visibility `{}`: {}", f.vis, e))
                    .to_compile_error()
            })?;
            // Encode the inner (unwrapped) type in brackets so the consuming
            // crate can restore `unwrapped_ty` and apply `Required` (`T!`).
            if let Some(ref inner) = f.unwrapped_ty {
                let inner_ts: TokenStream = inner.parse().map_err(|e| {
                    syn::Error::new(proc_macro2::Span::call_site(), format!("typeshaper: invalid inner type `{}`: {}", inner, e))
                        .to_compile_error()
                })?;
                Ok(quote! { #fvis #fname : #ftype [#inner_ts] })
            } else {
                Ok(quote! { #fvis #fname : #ftype })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(quote! {
        #input

        #[macro_export]
        macro_rules! #macro_ident {
            () => {
                ::typeshaper::__typeshaper_import!(#type_ident, #(#field_entries),*);
            };
        }
    })
}

/// Consume the output of `__typeshaper_import!`: parse inline field tokens and
/// register the type in the cross-crate import namespace. Emits no code.
///
/// Imported types are always registered under the `None` key (a dedicated
/// "cross-crate import" namespace), regardless of the current crate's
/// `CARGO_MANIFEST_DIR`.  This is intentional:
///
/// - rust-analyzer reuses a single proc-macro server for the whole workspace
///   and sometimes expands the nested proc-macro call produced by the
///   `typeshaper_import_TypeName!()` declarative macro with the *exporting*
///   crate's `CARGO_MANIFEST_DIR` rather than the *calling* crate's
///   directory.  Registering under a fixed `None` key sidesteps this
///   ambiguity entirely.
///
/// - Local types defined with `#[typeshaper]` or `#[typeshaper(export)]` always use
///   a `Some(crate_dir)` key, so they can never collide with the `None` key
///   used for imports.  `try_lookup` checks the precise crate key first and
///   only falls back to `None` when necessary, so a same-named local type
///   always takes priority over an imported one.
pub fn expand_import(input: ImportInput) -> TokenStream {
    let type_name = input.type_name.to_string();
    let fields: Vec<FieldDef> = input
        .fields
        .iter()
        .map(|(name, ty, unwrapped, vis)| {
            let vis_str = vis.to_string();
            if let Some(inner) = unwrapped {
                // Restore the Partial-wrapping metadata so `Required` (`T!`)
                // works correctly in this crate.
                FieldDef::wrapped_optional(
                    name.to_string(),
                    vis_str,
                    inner.to_string(),
                )
            } else {
                FieldDef::plain(name.to_string(), vis_str, ty.to_string())
            }
        })
        .collect();
    // Register in the import namespace (None key) — independent of
    // CARGO_MANIFEST_DIR so CARGO_MANIFEST_DIR mismatches in RA don't matter.
    register_import(type_name, fields);
    TokenStream::new()
}

// ---------------------------------------------------------------------------
// Recursive helpers
// ---------------------------------------------------------------------------

/// Expand one `ShapeExpr` into a `TokenStream` that defines `target`.
///
/// Any sub-expressions (composed `ShapeNode`s) are resolved by `expand_node`,
/// which generates anonymous intermediate types and appends them to `acc`.
fn expand_expr(
    expr: ShapeExpr,
    attrs: &[Attribute],
    target: Ident,
    hint: &str,
    acc: &mut Vec<TokenStream>,
) -> Result<TokenStream, TokenStream> {
    match expr {
        ShapeExpr::Rebuild { source } => {
            let source_ident = expand_node(source, hint, acc)?;
            rebuild(attrs, target, source_ident)
        }
        ShapeExpr::Omit { source, fields } => {
            let source_ident = expand_node(source, hint, acc)?;
            omit(attrs, target, source_ident, fields)
        }
        ShapeExpr::Pick { source, fields } => {
            let source_ident = expand_node(source, hint, acc)?;
            pick(attrs, target, source_ident, fields)
        }
        ShapeExpr::Merge { left, right } => {
            let left_ident  = expand_node(left,  hint, acc)?;
            let right_ident = expand_node(right, hint, acc)?;
            merge(attrs, target, left_ident, right_ident)
        }
        ShapeExpr::Partial { source } => {
            let source_ident = expand_node(source, hint, acc)?;
            partial(attrs, target, source_ident)
        }
        ShapeExpr::Required { source } => {
            let source_ident = expand_node(source, hint, acc)?;
            required(attrs, target, source_ident)
        }
        ShapeExpr::Diff { left, right } => {
            let left_ident  = expand_node(left,  hint, acc)?;
            let right_ident = expand_node(right, hint, acc)?;
            diff(attrs, target, left_ident, right_ident)
        }
    }
}

/// Resolve a `ShapeNode` to a concrete `Ident`.
///
/// - A leaf node returns the `Ident` directly.
/// - A composed node generates a fresh anonymous type name
///   (`__TypeshaperAnon_{hint}_{counter}`), recursively expands the inner
///   expression into `acc`, and returns the anonymous `Ident`.
fn expand_node(
    node: ShapeNode,
    hint: &str,
    acc: &mut Vec<TokenStream>,
) -> Result<Ident, TokenStream> {
    match node {
        ShapeNode::Leaf(ident) => Ok(ident),
        ShapeNode::Composed(expr) => {
            let id = next_anon_id();
            let anon_name = format!("__TypeshaperAnon_{}_{}", hint, id);
            let anon_ident: Ident =
                syn::parse_str(&anon_name).expect("anonymous ident is always valid");
            let ts = expand_expr(*expr, &[], anon_ident.clone(), hint, acc)?;
            acc.push(ts);
            Ok(anon_ident)
        }
    }
}

// ---------------------------------------------------------------------------
// T  →  Rebuild  (identical shape, new attribute set)
// ---------------------------------------------------------------------------

type R = Result<TokenStream, TokenStream>;

/// Re-emit every field of `source` unchanged, applying fresh `attrs`.
///
/// Primary use-case: attach different `#[derive(...)]` or other attributes to
/// an existing type without rewriting every field.
///
/// ```ignore
/// #[typeshaper]
/// #[derive(Debug, Clone)]
/// pub struct User { pub id: u64, pub name: String }
///
/// typex!(#[derive(Debug, Clone, Serialize, Deserialize)] UserDto = User);
/// ```
///
/// Generated impls: `TypeshaperInto<Target> for Source` (enables `.project()`).
fn rebuild(attrs: &[Attribute], target: Ident, source: Ident) -> R {
    let all = try_lookup(&source)?;
    let (names, types, vises) = to_token_vecs(&all)?;

    register(crate_key(), target.to_string(), all);

    Ok(quote! {
        #(#attrs)*
        pub struct #target {
            #(#vises #names: #types,)*
        }

        impl ::typeshaper::TypeshaperInto<#target> for #source {
            fn typeshaper_into(self) -> #target {
                #target { #(#names: self.#names,)* }
            }
        }
    })
}

fn omit(attrs: &[Attribute], target: Ident, source: Ident, omit_fields: Vec<Ident>) -> R {
    let all = try_lookup(&source)?;

    for f in &omit_fields {
        if !all.iter().any(|d| d.name == f.to_string()) {
            return Err(field_not_found(f, &source));
        }
    }

    let omit_set: std::collections::HashSet<String> =
        omit_fields.iter().map(|f| f.to_string()).collect();

    let kept: Vec<FieldDef> = all.into_iter().filter(|f| !omit_set.contains(&f.name)).collect();
    let (names, types, vises) = to_token_vecs(&kept)?;

    register(crate_key(), target.to_string(), kept);

    Ok(quote! {
        #(#attrs)*
        pub struct #target {
            #(#vises #names: #types,)*
        }

        impl ::typeshaper::TypeshaperInto<#target> for #source {
            fn typeshaper_into(self) -> #target {
                #target { #(#names: self.#names,)* }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// T & [fields]  →  Pick
// ---------------------------------------------------------------------------

fn pick(attrs: &[Attribute], target: Ident, source: Ident, pick_fields: Vec<Ident>) -> R {
    let all = try_lookup(&source)?;

    // Validate + preserve the order given by the caller.
    let mut seen = std::collections::HashSet::new();
    let mut kept: Vec<FieldDef> = Vec::new();
    for f in &pick_fields {
        let name = f.to_string();
        if !seen.insert(name.clone()) {
            return Err(syn::Error::new(
                f.span(),
                format!("field `{}` is listed more than once in the pick list", name),
            )
            .to_compile_error());
        }
        match all.iter().find(|d| d.name == name) {
            Some(d) => kept.push(d.clone()),
            None    => return Err(field_not_found(f, &source)),
        }
    }

    let (names, types, vises) = to_token_vecs(&kept)?;

    register(crate_key(), target.to_string(), kept);

    Ok(quote! {
        #(#attrs)*
        pub struct #target {
            #(#vises #names: #types,)*
        }

        impl ::typeshaper::TypeshaperInto<#target> for #source {
            fn typeshaper_into(self) -> #target {
                #target { #(#names: self.#names,)* }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// A + B  →  Merge
// ---------------------------------------------------------------------------

fn merge(attrs: &[Attribute], target: Ident, left: Ident, right: Ident) -> R {
    let fields_a = try_lookup(&left)?;
    let fields_b = try_lookup(&right)?;

    let names_a_set: std::collections::HashSet<&str> =
        fields_a.iter().map(|f| f.name.as_str()).collect();

    for f in &fields_b {
        if names_a_set.contains(f.name.as_str()) {
            return Err(syn::Error::new(
                right.span(),
                format!("field `{}` exists in both `{}` and `{}`", f.name, left, right),
            )
            .to_compile_error());
        }
    }

    let (names_a, types_a, vises_a) = to_token_vecs(&fields_a)?;
    let (names_b, types_b, vises_b) = to_token_vecs(&fields_b)?;

    let mut all = fields_a;
    all.extend(fields_b);
    register(crate_key(), target.to_string(), all);

    Ok(quote! {
        #(#attrs)*
        pub struct #target {
            #(#vises_a #names_a: #types_a,)*
            #(#vises_b #names_b: #types_b,)*
        }

        impl From<(#left, #right)> for #target {
            fn from((a, b): (#left, #right)) -> Self {
                #target {
                    #(#names_a: a.#names_a,)*
                    #(#names_b: b.#names_b,)*
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// T?  →  Partial  (every field becomes Option<_>)
// ---------------------------------------------------------------------------

fn partial(attrs: &[Attribute], target: Ident, source: Ident) -> R {
    let all = try_lookup(&source)?;

    // For each field decide how to handle it:
    //   • Already Option<_> (by a prior Partial or written by the user) → keep as-is.
    //     Record unwrapped_ty so that Required (`T!`) can restore the inner type later.
    //   • Plain field → wrap in Option<_>.
    //
    // This makes Partial idempotent: applying `T?` to a type that already has
    // optional fields never double-wraps or errors — it simply leaves those
    // fields unchanged.
    let mut opt_fields: Vec<FieldDef> = Vec::with_capacity(all.len());
    let mut already_opt: Vec<bool> = Vec::with_capacity(all.len());

    for f in &all {
        if f.unwrapped_ty.is_some() {
            // Tagged by a prior Partial — keep exactly as stored.
            opt_fields.push(f.clone());
            already_opt.push(true);
        } else if let Some(inner) = try_unwrap_option(&f.ty_tokens) {
            // Hand-written Option<T> field — preserve ty_tokens but record the
            // inner type in unwrapped_ty so Required can later unwrap it.
            opt_fields.push(FieldDef {
                name:         f.name.clone(),
                vis:          f.vis.clone(),
                ty_tokens:    f.ty_tokens.clone(),
                unwrapped_ty: Some(inner),
            });
            already_opt.push(true);
        } else {
            // Plain field — wrap in Option<_>.
            opt_fields.push(FieldDef::wrapped_optional(
                f.name.clone(),
                f.vis.clone(),
                f.ty_tokens.clone(),
            ));
            already_opt.push(false);
        }
    }

    let (names, opt_types, vises) = to_token_vecs(&opt_fields)?;

    register(crate_key(), target.to_string(), opt_fields);

    // Build per-field From expressions:
    //   already-Optional → pass through unchanged  (field: src.field)
    //   newly-wrapped    → wrap in Some             (field: Some(src.field))
    let from_exprs: Vec<TokenStream> = names
        .iter()
        .zip(already_opt.iter())
        .map(|(name, &already)| {
            if already {
                quote! { #name: src.#name }
            } else {
                quote! { #name: Some(src.#name) }
            }
        })
        .collect();

    Ok(quote! {
        #(#attrs)*
        pub struct #target {
            #(#vises #names: #opt_types,)*
        }

        impl From<#source> for #target {
            fn from(src: #source) -> Self {
                #target { #(#from_exprs,)* }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// T!  →  Required  (unwrap every Option<_> field added by Partial)
// ---------------------------------------------------------------------------

/// If `ty_tokens` represents `Option<T>`, returns the token-string of `T`.
///
/// Only recognises the standard-library `Option` in its three canonical forms:
/// - `Option<T>`
/// - `std::option::Option<T>`
/// - `core::option::Option<T>`
///
/// User-defined types whose last path segment happens to be `Option`
/// (e.g. `my_crate::Option<T>`) are intentionally **not** matched.
///
/// Handles both locally-created Partial types (which carry `unwrapped_ty`) and
/// imported types whose `unwrapped_ty` metadata was not available at import time
/// (e.g. pre-v0.3 exports or manually written structs with `Option<_>` fields).
fn try_unwrap_option(ty_tokens: &str) -> Option<String> {
    let ty: syn::Type = syn::parse_str(ty_tokens).ok()?;
    if let syn::Type::Path(tp) = ty {
        // Reject `<Foo as Trait>::Option` and similar qualified paths.
        if tp.qself.is_some() {
            return None;
        }
        let segs: Vec<_> = tp.path.segments.iter().collect();
        let is_std_option = match segs.as_slice() {
            [s] if s.ident == "Option" => true,
            [a, b, c]
                if (a.ident == "std" || a.ident == "core")
                    && b.ident == "option"
                    && c.ident == "Option" =>
            {
                true
            }
            _ => false,
        };
        if !is_std_option {
            return None;
        }
        let last = segs.last()?;
        if let syn::PathArguments::AngleBracketed(ref args) = last.arguments {
            if args.args.len() == 1 {
                if let syn::GenericArgument::Type(inner) = &args.args[0] {
                    return Some(quote!(#inner).to_string());
                }
            }
        }
    }
    None
}

fn required(attrs: &[Attribute], target: Ident, source: Ident) -> R {
    let all = try_lookup(&source)?;

    // For each field decide how to handle it:
    //   • Option<_> field (by Partial or user-written) → unwrap to inner type.
    //   • Plain field → keep as-is.
    //
    // This makes Required complementary to Partial but still graceful:
    // non-Option fields are preserved unchanged, so `T!` can be applied to
    // mixed types without errors.
    let inner_fields: Vec<FieldDef> = all
        .iter()
        .map(|f| {
            let inner_ty = f.unwrapped_ty.clone()
                .or_else(|| try_unwrap_option(&f.ty_tokens));
            match inner_ty {
                Some(inner) => FieldDef::plain(f.name.clone(), f.vis.clone(), inner),
                None        => f.clone(), // not Option<_> — keep as-is
            }
        })
        .collect();

    // Track which fields are Option<_> (need `.ok_or()` in TryFrom) vs plain.
    let needs_unwrap: Vec<bool> = all
        .iter()
        .map(|f| f.unwrapped_ty.is_some() || try_unwrap_option(&f.ty_tokens).is_some())
        .collect();

    let (names, inner_types, vises) = to_token_vecs(&inner_fields)?;

    register(crate_key(), target.to_string(), inner_fields);

    let any_needs_unwrap = needs_unwrap.iter().any(|&b| b);

    if any_needs_unwrap {
        // At least one field is Option<_> and can be None → use TryFrom.
        //
        // Build per-field TryFrom expressions:
        //   Option<_> field → src.field.ok_or(RequiredError { field: "…" })?
        //   plain field     → src.field
        let try_from_exprs: Vec<TokenStream> = names
            .iter()
            .zip(needs_unwrap.iter())
            .map(|(name, &unwrap)| {
                if unwrap {
                    quote! {
                        #name: src.#name.ok_or(
                            ::typeshaper::RequiredError::new(stringify!(#name))
                        )?
                    }
                } else {
                    quote! { #name: src.#name }
                }
            })
            .collect();

        Ok(quote! {
            #(#attrs)*
            pub struct #target {
                #(#vises #names: #inner_types,)*
            }

            impl TryFrom<#source> for #target {
                type Error = ::typeshaper::RequiredError;

                fn try_from(src: #source) -> Result<Self, Self::Error> {
                    Ok(Self { #(#try_from_exprs,)* })
                }
            }
        })
    } else {
        // No field is Option<_> — conversion can never fail.  Generate From
        // instead: the standard-library blanket impl
        //   `impl<T, U> TryFrom<U> for T where T: From<U>`
        // automatically provides TryFrom as well, so existing call-sites using
        // `Target::try_from(src)` continue to compile without change.
        Ok(quote! {
            #(#attrs)*
            pub struct #target {
                #(#vises #names: #inner_types,)*
            }

            impl From<#source> for #target {
                fn from(src: #source) -> Self {
                    Self { #(#names: src.#names,)* }
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// A % B  →  Diff  (fields present in A but absent in B)
// ---------------------------------------------------------------------------

fn diff(attrs: &[Attribute], target: Ident, left: Ident, right: Ident) -> R {
    let fields_a = try_lookup(&left)?;
    let fields_b = try_lookup(&right)?;

    // Keep fields whose (name, type) pair does not appear in B.
    // Comparing by type string prevents excluding A.id:u64 just because
    // B happens to have an unrelated id:String field.
    //
    // Types are normalised before comparison: `quote!(#ty).to_string()` from
    // `extract_named_fields` and `format!("Option<{}>", …)` from
    // `wrapped_optional` can produce different whitespace for the same type.
    // `normalize_ty` round-trips both through syn→quote to a canonical form.
    let kept: Vec<FieldDef> = fields_a
        .into_iter()
        .filter(|f| {
            !fields_b.iter().any(|fb| {
                fb.name == f.name && normalize_ty(&fb.ty_tokens) == normalize_ty(&f.ty_tokens)
            })
        })
        .collect();

    if kept.is_empty() {
        return Err(syn::Error::new(
            left.span(),
            format!("`{}` has no fields that are absent from `{}`", left, right),
        )
        .to_compile_error());
    }

    let (names, types, vises) = to_token_vecs(&kept)?;

    register(crate_key(), target.to_string(), kept);

    Ok(quote! {
        #(#attrs)*
        pub struct #target {
            #(#vises #names: #types,)*
        }

        impl ::typeshaper::TypeshaperInto<#target> for #left {
            fn typeshaper_into(self) -> #target {
                #target { #(#names: self.#names,)* }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Canonicalise a type-token string for equality comparisons.
///
/// Different code paths store the same conceptual type with different
/// whitespace: `quote!(#ty).to_string()` yields `"Option < String >"` while
/// `format!("Option<{}>", inner)` yields `"Option<String>"`.  Parsing the
/// string with syn and re-serialising via `quote!` normalises both to the
/// same representation, making type comparisons robust across storage paths.
///
/// Falls back to the original string if parsing fails (defensive; should not
/// happen for well-formed stored types).
fn normalize_ty(ty_tokens: &str) -> String {
    syn::parse_str::<syn::Type>(ty_tokens)
        .map(|ty| quote!(#ty).to_string())
        .unwrap_or_else(|_| ty_tokens.to_owned())
}

fn try_lookup(ident: &Ident) -> Result<Vec<FieldDef>, TokenStream> {
    let file = crate_key();
    let name = ident.to_string();

    // 1. Precise lookup — locally-defined types registered by `#[typeshaper]` or
    //    `#[typeshaper(export)]` under the current crate's `CARGO_MANIFEST_DIR`.
    //    Always taken for `cargo build` / `cargo test`.
    if let Some(fields) = lookup(file, &name) {
        return Ok(fields);
    }

    // 2. Import-namespace fallback — types registered by `__typeshaper_import!`
    //    (via `typeshaper_import_TypeName!()`).  Stored under the `None` key so
    //    they are immune to `CARGO_MANIFEST_DIR` mismatches in rust-analyzer.
    //    Locally-defined types are never stored under `None`, so this cannot
    //    accidentally match a same-named type from another crate.
    if let Some(fields) = lookup(None, &name) {
        return Ok(fields);
    }

    // 3. Export-registry fallback — types that carry `#[typeshaper(export)]` are
    //    written to a dedicated export registry at their own crate's compile
    //    time.  rust-analyzer processes dependency crates before the current
    //    crate, so this entry is always present when `typex!()` in a consuming
    //    crate is expanded — even if `typeshaper_import_TypeName!()` was not yet
    //    expanded (which can happen when the nested declarative→proc-macro call
    //    chain is deferred by rust-analyzer's lazy expander).
    //
    //    This fallback is safe from collisions: `#[typeshaper]` (non-exported,
    //    local types) does NOT write to the export registry, so a same-named
    //    local type in another crate cannot be picked up here.
    if let Some(fields) = lookup_exported(&name) {
        return Ok(fields);
    }

    Err(syn::Error::new(
        ident.span(),
        format!(
            "type `{}` is not registered — \
             apply `#[typeshaper]` to it, or create it with `typex!()` first",
            ident
        ),
    )
    .to_compile_error())
}

fn field_not_found(field: &Ident, source: &Ident) -> TokenStream {
    syn::Error::new(
        field.span(),
        format!("field `{}` does not exist on `{}`", field, source),
    )
    .to_compile_error()
}

fn extract_named_fields(input: &DeriveInput) -> Result<Vec<FieldDef>, TokenStream> {
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => Ok(named
                .named
                .iter()
                .filter_map(|f| {
                    f.ident.as_ref().map(|ident| {
                        let ty  = &f.ty;
                        let vis = &f.vis;
                        FieldDef::plain(
                            ident.to_string(),
                            quote!(#vis).to_string(),
                            quote!(#ty).to_string(),
                        )
                    })
                })
                .collect()),
            _ => Err(syn::Error::new_spanned(
                &input.ident,
                "#[typeshaper] only supports structs with named fields",
            )
            .to_compile_error()),
        },
        _ => Err(
            syn::Error::new_spanned(&input.ident, "#[typeshaper] only supports structs")
                .to_compile_error(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ImportInput;
    use quote::quote;

    /// 验证 expand_import 正确保留字段可见性。
    ///
    /// `ImportInput` 携带 4-tuple `(name, ty, unwrapped, vis)`，第四槽位是可见性。
    /// `expand_import` 应将 vis 原样写入 FieldDef.vis，不得硬编码为 "pub"。
    #[test]
    fn issue1_import_preserves_field_visibility() {
        let input = ImportInput {
            type_name: syn::parse_str("__BugIssue1_VisLoss").unwrap(),
            fields: vec![
                // pub id: u64（公开字段）
                (syn::parse_str("id").unwrap(), quote! { u64 }, None, quote! { pub }),
                // secret: String（私有字段，空 visibility）
                (syn::parse_str("secret").unwrap(), quote! { String }, None, quote! {}),
            ],
        };

        expand_import(input);

        let fields = crate::state::lookup(None, "__BugIssue1_VisLoss")
            .expect("expand_import 应将类型注册到 None 命名空间");

        let id_vis = &fields.iter().find(|f| f.name == "id").unwrap().vis;
        let secret_vis = &fields.iter().find(|f| f.name == "secret").unwrap().vis;

        assert_eq!(id_vis, "pub", "公开字段可见性应为 pub");
        assert_eq!(secret_vis, "", "私有字段跨 crate 导入后应保留空可见性（无 pub 前缀）");
    }
}

fn to_token_vecs(
    fields: &[FieldDef],
) -> Result<(Vec<Ident>, Vec<TokenStream>, Vec<TokenStream>), TokenStream> {
    let mut names = Vec::with_capacity(fields.len());
    let mut types = Vec::with_capacity(fields.len());
    let mut vises = Vec::with_capacity(fields.len());
    for f in fields {
        let ident = syn::parse_str::<Ident>(&f.name).map_err(|e| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("typeshaper: invalid field name `{}`: {}", f.name, e),
            )
            .to_compile_error()
        })?;
        let ty = f.ty_tokens.parse::<TokenStream>().map_err(|e| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("typeshaper: invalid type string `{}`: {}", f.ty_tokens, e),
            )
            .to_compile_error()
        })?;
        let vis: TokenStream = f.vis.parse().map_err(|e| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("typeshaper: invalid visibility `{}`: {}", f.vis, e),
            )
            .to_compile_error()
        })?;
        names.push(ident);
        types.push(ty);
        vises.push(vis);
    }
    Ok((names, types, vises))
}
