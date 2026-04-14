use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Ident, Result, Token, bracketed, parenthesized};

/// The full input to `typex!( [#[attr...]] Target = Expr )`.
pub struct ShapeInput {
    /// Outer attributes placed before the target name, e.g. `#[derive(Serialize)]`.
    pub attrs: Vec<Attribute>,
    pub target: Ident,
    pub expr: ShapeExpr,
}

/// A node in the type-algebra expression tree.
///
/// Either a leaf (a registered type name) or a composed sub-expression
/// produced by a parenthesised group.
pub enum ShapeNode {
    Leaf(Ident),
    Composed(Box<ShapeExpr>),
}

/// A single type-algebra operation.
///
/// Sources and operands are `ShapeNode`s, so any position that previously
/// accepted only an `Ident` can now accept a full parenthesised expression.
///
/// | Syntax              | Variant   | Meaning                            |
/// |---------------------|-----------|------------------------------------|
/// | Syntax              | Variant   | Meaning                            |
/// |---------------------|-----------|------------------------------------|
/// | `T`                 | Rebuild   | Clone struct shape, add new attrs  |
/// | `T - [f1, f2]`      | Omit      | Drop listed fields                 |
/// | `T & [f1, f2]`      | Pick      | Keep only listed fields            |
/// | `A + B`             | Merge     | Combine all fields of A and B      |
/// | `T?`                | Partial   | Wrap every field in `Option<_>`    |
/// | `T!`                | Required  | Unwrap every `Option<_>` field     |
/// | `A % B`             | Diff      | Fields in A that do not exist in B |
pub enum ShapeExpr {
    Rebuild  { source: ShapeNode },
    Omit     { source: ShapeNode, fields: Vec<Ident> },
    Pick     { source: ShapeNode, fields: Vec<Ident> },
    Merge    { left: ShapeNode, right: ShapeNode },
    Partial  { source: ShapeNode },
    Required { source: ShapeNode },
    Diff     { left: ShapeNode, right: ShapeNode },
}

impl Parse for ShapeInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let attrs = Attribute::parse_outer(input)?;
        let target: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let node = parse_expr_as_node(input)?;
        match node {
            // Bare source with no operator → Rebuild: copy all fields, apply
            // new attributes.  Useful for attaching derives like Serialize to
            // an existing type without repeating field definitions.
            ShapeNode::Leaf(ident) => Ok(Self {
                attrs,
                target,
                expr: ShapeExpr::Rebuild { source: ShapeNode::Leaf(ident) },
            }),
            ShapeNode::Composed(expr) => Ok(Self { attrs, target, expr: *expr }),
        }
    }
}

/// Parse `Atom Tail*` into a `ShapeNode`, accumulating operations left-to-right.
///
/// Each `Tail` wraps the current `lhs` node into a new `ShapeNode::Composed`.
/// An `Atom` with no tails returns a bare `ShapeNode::Leaf`.
fn parse_expr_as_node(input: ParseStream) -> Result<ShapeNode> {
    let mut lhs = parse_atom(input)?;

    loop {
        // `lhs - [fields]`  →  Omit
        if input.peek(Token![-]) {
            input.parse::<Token![-]>()?;
            let fields = parse_ident_list(input)?;
            lhs = ShapeNode::Composed(Box::new(ShapeExpr::Omit { source: lhs, fields }));
            continue;
        }

        // `lhs & [fields]`  →  Pick
        if input.peek(Token![&]) {
            input.parse::<Token![&]>()?;
            let fields = parse_ident_list(input)?;
            lhs = ShapeNode::Composed(Box::new(ShapeExpr::Pick { source: lhs, fields }));
            continue;
        }

        // `lhs + Atom`  →  Merge  (right side is Atom only)
        if input.peek(Token![+]) {
            input.parse::<Token![+]>()?;
            let right = parse_atom(input)?;
            lhs = ShapeNode::Composed(Box::new(ShapeExpr::Merge { left: lhs, right }));
            continue;
        }

        // `lhs?`  →  Partial
        if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            lhs = ShapeNode::Composed(Box::new(ShapeExpr::Partial { source: lhs }));
            continue;
        }

        // `lhs!`  →  Required
        if input.peek(Token![!]) {
            input.parse::<Token![!]>()?;
            lhs = ShapeNode::Composed(Box::new(ShapeExpr::Required { source: lhs }));
            continue;
        }

        // `lhs % Atom`  →  Diff  (right side is Atom only)
        if input.peek(Token![%]) {
            input.parse::<Token![%]>()?;
            let right = parse_atom(input)?;
            lhs = ShapeNode::Composed(Box::new(ShapeExpr::Diff { left: lhs, right }));
            continue;
        }

        break;
    }

    Ok(lhs)
}

/// Parse a single `Atom`: either a parenthesised expression or a bare `Ident`.
fn parse_atom(input: ParseStream) -> Result<ShapeNode> {
    if input.peek(syn::token::Paren) {
        let content;
        parenthesized!(content in input);
        parse_expr_as_node(&content)
    } else {
        let ident: Ident = input.parse()?;
        Ok(ShapeNode::Leaf(ident))
    }
}

fn parse_ident_list(input: ParseStream) -> Result<Vec<Ident>> {
    let content;
    bracketed!(content in input);
    Ok(Punctuated::<Ident, Token![,]>::parse_terminated(&content)?
        .into_iter()
        .collect())
}

// ---------------------------------------------------------------------------
// Input for `__typeshaper_import!( TypeName, field: Type, ... )`
// ---------------------------------------------------------------------------

/// Parsed input for the internal `__typeshaper_import!` proc-macro.
///
/// Format: `TypeName, [vis] field1: Type1, [vis] field2: Type2 [InnerType2], ...`
///
/// The optional visibility token(s) before a field name (`pub`, `pub(crate)`, …)
/// preserve the original field visibility across the crate boundary.
///
/// The optional `[InnerType]` bracket after a field's type carries the
/// `unwrapped_ty` metadata produced by a `Partial` (`T?`) operation, allowing
/// `Required` (`T!`) to work correctly in consuming crates.
///
/// Produced by the companion `typeshaper_import_TypeName!()` macro generated by
/// `#[typeshaper(export)]`. Users never write this directly.
pub struct ImportInput {
    pub type_name: Ident,
    /// `(field_name, ty_tokens, unwrapped_inner_ty, vis_tokens)`
    ///
    /// `unwrapped_inner_ty` is `Some` when the exporting crate encoded the
    /// original pre-`Option` type in brackets (e.g. `[u64]` after `Option<u64>`).
    ///
    /// `vis_tokens` is the field's visibility as a token stream (`pub`,
    /// `pub (crate)`, or empty for private).
    pub fields: Vec<(Ident, TokenStream, Option<TokenStream>, TokenStream)>,
}

impl Parse for ImportInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let type_name: Ident = input.parse()?;
        let mut fields = Vec::new();
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            // Parse optional visibility (pub / pub(crate) / …) before field name.
            // syn::Visibility is non-consuming when the next token is not a
            // visibility keyword, so private fields simply yield Visibility::Inherited.
            let vis: syn::Visibility = input.parse()?;
            let name: Ident = input.parse()?;
            input.parse::<Token![:]>()?;
            let ty: syn::Type = input.parse()?;
            let unwrapped = if input.peek(syn::token::Bracket) {
                let content;
                bracketed!(content in input);
                let inner: syn::Type = content.parse()?;
                Some(quote! { #inner })
            } else {
                None
            };
            fields.push((name, quote! { #ty }, unwrapped, quote! { #vis }));
        }
        Ok(Self { type_name, fields })
    }
}
