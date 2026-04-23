use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Ident, Result, Token, bracketed, parenthesized};

/// The full input to `typex!( [#[attr...]] [vis] Target[<Generics>][where ...] = Expr )`.
pub struct ShapeInput {
    /// Outer attributes placed before the target name, e.g. `#[derive(Serialize)]`.
    pub attrs: Vec<Attribute>,
    /// Visibility of the generated struct.
    /// Defaults to `Inherited` (private) when no visibility keyword is written.
    pub vis: syn::Visibility,
    pub target: Ident,
    /// Explicit generic parameters for the generated target type.
    ///
    /// Written as `Target<T: Clone, U>` or `Target<T> where T: Clone`.
    /// If absent, code generation falls back to auto-inheriting the source's
    /// registered generics (backward compat for non-generic sources).
    pub target_generics: syn::Generics,
    pub expr: ShapeExpr,
}

/// A node in the type-algebra expression tree.
pub enum ShapeNode {
    /// A named registered type with optional explicit type arguments.
    ///
    /// `User` → `Leaf(User, None)`
    /// `User<T>` → `Leaf(User, Some(<T>))`
    /// `Pair<A, B>` → `Leaf(Pair, Some(<A, B>))`
    Leaf(Ident, Option<TokenStream>),
    Composed(Box<ShapeExpr>),
}

/// A single type-algebra operation.
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

        // Parse optional visibility: `pub`, `pub(crate)`, `pub(super)`, etc.
        // If no visibility keyword is present, `syn::Visibility::Inherited` is returned,
        // which generates a private struct (Rust's default).
        let vis: syn::Visibility = input.parse()?;

        let target: Ident = input.parse()?;

        // Parse optional generic params: `<T: Clone, 'a, U>`.
        // `syn::Generics::parse` peeks at `<`; if absent it returns empty generics.
        let mut target_generics: syn::Generics = input.parse()?;

        // Parse optional where clause: `where T: Debug`.
        target_generics.where_clause = if input.peek(Token![where]) {
            Some(input.parse()?)
        } else {
            None
        };

        input.parse::<Token![=]>()?;

        let node = parse_expr_as_node(input)?;
        match node {
            ShapeNode::Leaf(ident, ty_args) => Ok(Self {
                attrs,
                vis,
                target,
                target_generics,
                expr: ShapeExpr::Rebuild { source: ShapeNode::Leaf(ident, ty_args) },
            }),
            ShapeNode::Composed(expr) => Ok(Self { attrs, vis, target, target_generics, expr: *expr }),
        }
    }
}

/// Parse `Atom Tail*` into a `ShapeNode`, accumulating operations left-to-right.
fn parse_expr_as_node(input: ParseStream) -> Result<ShapeNode> {
    let mut lhs = parse_atom(input)?;

    loop {
        if input.peek(Token![-]) {
            input.parse::<Token![-]>()?;
            let fields = parse_ident_list(input)?;
            lhs = ShapeNode::Composed(Box::new(ShapeExpr::Omit { source: lhs, fields }));
            continue;
        }
        if input.peek(Token![&]) {
            input.parse::<Token![&]>()?;
            let fields = parse_ident_list(input)?;
            lhs = ShapeNode::Composed(Box::new(ShapeExpr::Pick { source: lhs, fields }));
            continue;
        }
        if input.peek(Token![+]) {
            input.parse::<Token![+]>()?;
            let right = parse_atom(input)?;
            lhs = ShapeNode::Composed(Box::new(ShapeExpr::Merge { left: lhs, right }));
            continue;
        }
        if input.peek(Token![?]) {
            input.parse::<Token![?]>()?;
            lhs = ShapeNode::Composed(Box::new(ShapeExpr::Partial { source: lhs }));
            continue;
        }
        if input.peek(Token![!]) {
            input.parse::<Token![!]>()?;
            lhs = ShapeNode::Composed(Box::new(ShapeExpr::Required { source: lhs }));
            continue;
        }
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

/// Parse a single `Atom`: either a parenthesised expression or a bare `Ident`
/// optionally followed by angle-bracket type arguments.
///
/// `User`       → `Leaf(User, None)`
/// `User<T>`    → `Leaf(User, Some(<T>))`
/// `(expr...)`  → `Composed(...)`
fn parse_atom(input: ParseStream) -> Result<ShapeNode> {
    if input.peek(syn::token::Paren) {
        let content;
        parenthesized!(content in input);
        return parse_expr_as_node(&content);
    }
    let ident: Ident = input.parse()?;
    // Try to parse explicit angle-bracket type args: `<T>`, `<'a, T: Clone>`, etc.
    // `AngleBracketedGenericArguments` handles balanced `<...>` correctly.
    let ty_args: Option<TokenStream> = if input.peek(Token![<]) {
        let args: syn::AngleBracketedGenericArguments = input.parse()?;
        Some(quote!(#args))
    } else {
        None
    };
    Ok(ShapeNode::Leaf(ident, ty_args))
}

fn parse_ident_list(input: ParseStream) -> Result<Vec<Ident>> {
    let content;
    bracketed!(content in input);
    Ok(Punctuated::<Ident, Token![,]>::parse_terminated(&content)?
        .into_iter()
        .collect())
}

// ---------------------------------------------------------------------------
// Input for `__typeshaper_import!`
// ---------------------------------------------------------------------------
//
// Wire format (v2, since typeshaper 0.1.4):
//   TypeName, [<GenericParams>], [WhereClause], [vis] field1: Type1, ...
//
// The two bracket groups encode generics and where clause respectively.
// Empty brackets `[]` denote absent generics / where clause.

pub struct ImportInput {
    pub type_name: Ident,
    /// Token stream of the angle-bracket params, e.g. `<T : Clone>`.
    /// Empty if the type has no generic parameters.
    pub generics_tokens: TokenStream,
    /// Token stream of the where clause, e.g. `where T : Clone`.
    /// Empty if there is no where clause.
    pub where_tokens: TokenStream,
    /// `(field_name, ty_tokens, unwrapped_inner_ty, vis_tokens)`
    pub fields: Vec<(Ident, TokenStream, Option<TokenStream>, TokenStream)>,
}

impl Parse for ImportInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let type_name: Ident = input.parse()?;
        input.parse::<Token![,]>()?;

        // First bracket group: `[<T: Clone>]` or `[]`.
        let generics_tokens = {
            let content;
            bracketed!(content in input);
            content.parse::<TokenStream>()?
        };
        input.parse::<Token![,]>()?;

        // Second bracket group: `[where T: Clone]` or `[]`.
        let where_tokens = {
            let content;
            bracketed!(content in input);
            content.parse::<TokenStream>()?
        };

        // Fields (same format as before).
        let mut fields = Vec::new();
        while !input.is_empty() {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
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
        Ok(Self { type_name, generics_tokens, where_tokens, fields })
    }
}
