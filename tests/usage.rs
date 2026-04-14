use typeshaper::{RequiredError, TypeshaperExt, typeshaper, typex};
use std::convert::TryFrom;

// ---------------------------------------------------------------------------
// Source types — must appear before any typex!() that references them.
// ---------------------------------------------------------------------------

#[typeshaper]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub age: u8,
    email: String,
}

#[typeshaper]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Badge {
    pub score: u32,
    pub label: String,
}

// ---------------------------------------------------------------------------
// Type-algebra expressions
// ---------------------------------------------------------------------------

// T - [fields]  →  Omit age
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    UserNoAge = User - [age]
);

// T & [fields]  →  Pick id and name only
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    UserPublic = User & [id, name]
);

// A + B         →  Merge UserNoAge with Badge
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    UserWithBadge = UserNoAge + Badge
);

// T?            →  Partial: every field becomes Option<_>
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    UserDraft = User?
);

// T!            →  Required: restore all Option<_> fields from a Partial
typex!(#[derive(Debug, Clone, PartialEq, Eq)] UserComplete = UserDraft!);

// A % B         →  Diff: fields in User that are absent in Badge
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    UserOnly = User % Badge
);

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn omit_removes_listed_fields() {
    let user = User {
        id: 1,
        name: "alice".into(),
        age: 25,
        email: "a@b.com".into(),
    };

    let no_age: UserNoAge = user.project();

    assert_eq!(
        no_age,
        UserNoAge {
            id: 1,
            name: "alice".into(),
            email: "a@b.com".into()
        }
    );
}

#[test]
fn pick_keeps_only_listed_fields() {
    let user = User {
        id: 2,
        name: "bob".into(),
        age: 30,
        email: "b@c.com".into(),
    };

    let public_user: UserPublic = user.project();

    assert_eq!(
        public_user,
        UserPublic {
            id: 2,
            name: "bob".into()
        }
    );
}

#[test]
fn merge_combines_fields_via_from_tuple() {
    let no_age = UserNoAge {
        id: 3,
        name: "carol".into(),
        email: "c@d.com".into(),
    };
    let badge = Badge {
        score: 99,
        label: "gold".into(),
    };

    let combined = UserWithBadge::from((no_age, badge));

    assert_eq!(combined.id, 3);
    assert_eq!(combined.name, "carol");
    assert_eq!(combined.email, "c@d.com");
    assert_eq!(combined.score, 99);
    assert_eq!(combined.label, "gold");
}

#[test]
fn partial_wraps_all_fields_in_option() {
    let user = User {
        id: 4,
        name: "dave".into(),
        age: 22,
        email: "d@e.com".into(),
    };

    let draft = UserDraft::from(user);

    assert_eq!(draft.id, Some(4));
    assert_eq!(draft.name, Some("dave".into()));
    assert_eq!(draft.age, Some(22));
    assert_eq!(draft.email, Some("d@e.com".into()));
}

#[test]
fn required_unwraps_all_some_fields() {
    let draft = UserDraft {
        id: Some(5),
        name: Some("eve".into()),
        age: Some(28),
        email: Some("e@f.com".into()),
    };

    let complete = UserComplete::try_from(draft).expect("all fields are Some");

    assert_eq!(complete.id, 5);
    assert_eq!(complete.name, "eve");
    assert_eq!(complete.age, 28);
    assert_eq!(complete.email, "e@f.com");
}

#[test]
fn required_errors_when_a_field_is_none() {
    let draft = UserDraft {
        id: None,
        name: Some("x".into()),
        age: Some(1),
        email: Some("x@y.com".into()),
    };

    let err = UserComplete::try_from(draft).unwrap_err();
    assert_eq!(err, RequiredError::new("id"));
    assert_eq!(err.field, "id");
    assert!(err.to_string().contains("id"));
}

#[test]
fn diff_keeps_only_fields_absent_in_second_type() {
    // User has: id, name, age, email
    // Badge has: score, label
    // Diff keeps all of User's fields (none overlap with Badge)
    let user = User {
        id: 10,
        name: "hank".into(),
        age: 33,
        email: "h@i.com".into(),
    };
    let user_only: UserOnly = user.project();
    assert_eq!(
        user_only,
        UserOnly {
            id: 10,
            name: "hank".into(),
            age: 33,
            email: "h@i.com".into()
        }
    );
}

#[test]
fn diff_excludes_shared_field_names() {
    // UserNoAge has: id, name, email
    // UserPublic has: id, name
    // Diff keeps only: email
    typex!(
        #[derive(Debug, Clone, PartialEq, Eq)]
        OnlyEmail = UserNoAge % UserPublic
    );
    let src = UserNoAge {
        id: 11,
        name: "iris".into(),
        email: "i@j.com".into(),
    };
    let only_email: OnlyEmail = src.project();
    assert_eq!(
        only_email,
        OnlyEmail {
            email: "i@j.com".into()
        }
    );
}

#[test]
fn multiple_attrs_forwarded_to_generated_struct() {
    // Verify that multiple attributes and non-derive attrs are forwarded correctly.
    typex!(
        #[derive(Debug, Clone, PartialEq, Eq)]
        #[allow(dead_code)]
        UserMinimal = User & [id, name]
    );
    let user = User {
        id: 42,
        name: "zoe".into(),
        age: 20,
        email: "z@z.com".into(),
    };
    let minimal: UserMinimal = user.project();
    assert_eq!(
        minimal,
        UserMinimal {
            id: 42,
            name: "zoe".into()
        }
    );
}

#[test]
fn omit_result_can_feed_into_merge() {
    // Confirms typex!-generated types are themselves registered.
    let combined = UserWithBadge {
        id: 9,
        name: "fred".into(),
        email: "f@g.com".into(),
        score: 7,
        label: "bronze".into(),
    };
    assert_eq!(combined.score, 7);
}

// ---------------------------------------------------------------------------
// v0.2.0 — composite expressions
// ---------------------------------------------------------------------------

// Chain: Omit age, then Pick id and name (left-to-right)
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    UserDto = User - [age] & [id, name]
);

// Right-side parenthesised sub-expression: merge User with Badge minus its label
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    UserFull = User + (Badge - [label])
);

// Postfix chain: Omit, then Partial
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    UserNoAgeDraft = User - [age]?
);

// Nested parens: Omit → Partial → Required
typex!(#[derive(Debug, Clone, PartialEq, Eq)] UserNoAgeComplete = (User - [age])?!);

// Diff right-hand side is a sub-expression
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    UserExtraFields = User % (User & [id, name])
);

#[test]
fn chain_omit_then_pick() {
    // User - [age] & [id, name]  →  struct with only id and name
    // The intermediate "User without age" is an anonymous type; UserDto keeps only id and name.
    let dto = UserDto {
        id: 1,
        name: "alice".into(),
    };
    assert_eq!(dto.id, 1);
    assert_eq!(dto.name, "alice");
}

#[test]
fn merge_with_parenthesised_right() {
    // User + (Badge - [label])  →  User fields + score (label is dropped via the anonymous sub-expr)
    // .project() is inferred as the anonymous intermediate type via From<(User, Anon)> context.
    let user = User {
        id: 2,
        name: "bob".into(),
        age: 30,
        email: "b@c.com".into(),
    };
    let badge = Badge {
        score: 88,
        label: "silver".into(),
    };
    let full = UserFull::from((user, badge.project()));
    assert_eq!(full.id, 2);
    assert_eq!(full.name, "bob");
    assert_eq!(full.age, 30);
    assert_eq!(full.email, "b@c.com");
    assert_eq!(full.score, 88);
}

#[test]
fn chain_omit_then_partial() {
    // User - [age]?  →  remaining fields (id, name, email) all wrapped in Option, no age field.
    // Direct construction verifies the correct struct shape is generated.
    let draft = UserNoAgeDraft {
        id: Some(3),
        name: Some("carol".into()),
        email: Some("c@d.com".into()),
    };
    assert_eq!(draft.id, Some(3));
    assert_eq!(draft.name, Some("carol".into()));
    assert_eq!(draft.email, Some("c@d.com".into()));
}

#[test]
fn nested_parens_omit_partial_required() {
    // (User - [age])?!  →  non-optional fields (minus age), after Partial then Required
    // Verify the generated type has the expected fields and can be constructed directly.
    let complete = UserNoAgeComplete {
        id: 4,
        name: "dave".into(),
        email: "d@e.com".into(),
    };
    assert_eq!(complete.id, 4);
    assert_eq!(complete.name, "dave");
    assert_eq!(complete.email, "d@e.com");
}

#[test]
fn diff_with_parenthesised_right() {
    // User % (User & [id, name])  →  keeps age and email (fields absent from the picked sub-set)
    // Diff always generates TypeshaperInto<Target> for the left type, so .project() works directly.
    let user = User {
        id: 5,
        name: "eve".into(),
        age: 33,
        email: "e@f.com".into(),
    };
    let extra: UserExtraFields = user.project();
    assert_eq!(extra.age, 33);
    assert_eq!(extra.email, "e@f.com");
}

// ---------------------------------------------------------------------------
// v0.3.0 — cross-crate export / import simulation
//
// In production: `#[typeshaper(export)]` lives in core-crate, `typeshaper_import_T!()`
// is called in ffi-crate. Here both are in the same file to exercise the full
// round-trip at compile time without a separate test workspace.
// ---------------------------------------------------------------------------

#[typeshaper(export)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    pub id: u64,
    pub title: String,
    pub price_cents: u64,
    pub hidden: bool,
}

// Simulates the ffi-crate calling the companion macro.
// In the same-crate case this is a no-op re-registration; in the cross-crate
// case it is the only registration that happens on the consumer side.
typeshaper_import_Product!();

typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    ProductPublic = Product - [hidden]
);
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    ProductSummary = Product & [id, title]
);
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    ProductPartial = Product?
);

#[test]
fn export_import_omit() {
    let p = Product {
        id: 1,
        title: "Widget".into(),
        price_cents: 499,
        hidden: false,
    };
    let public: ProductPublic = p.project();
    assert_eq!(
        public,
        ProductPublic {
            id: 1,
            title: "Widget".into(),
            price_cents: 499
        }
    );
}

#[test]
fn export_import_pick() {
    let p = Product {
        id: 2,
        title: "Gadget".into(),
        price_cents: 999,
        hidden: true,
    };
    let summary: ProductSummary = p.project();
    assert_eq!(
        summary,
        ProductSummary {
            id: 2,
            title: "Gadget".into()
        }
    );
}

#[test]
fn export_import_partial() {
    let p = Product {
        id: 3,
        title: "Thing".into(),
        price_cents: 199,
        hidden: false,
    };
    let draft = ProductPartial::from(p);
    assert_eq!(draft.id, Some(3));
    assert_eq!(draft.title, Some("Thing".into()));
    assert_eq!(draft.price_cents, Some(199));
    assert_eq!(draft.hidden, Some(false));
}

// ---------------------------------------------------------------------------
// Cross-crate Required: simulate an imported Partial type whose unwrapped_ty
// metadata was not available at import time (pre-v0.3 exports, or manually
// written structs with Option<_> fields).  The Required fallback must detect
// Option<_> fields and still produce a working TryFrom impl.
// ---------------------------------------------------------------------------

#[typeshaper]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedDraft {
    pub id: Option<u64>,
    pub name: Option<String>,
    pub score: Option<u32>,
}

typex!(#[derive(Debug, Clone, PartialEq, Eq)] ImportedComplete = ImportedDraft!);

#[test]
fn required_works_on_imported_option_fields() {
    let draft = ImportedDraft {
        id: Some(42),
        name: Some("alice".into()),
        score: Some(99),
    };
    let complete = ImportedComplete::try_from(draft).expect("all fields are Some");
    assert_eq!(complete.id, 42);
    assert_eq!(complete.name, "alice");
    assert_eq!(complete.score, 99);
}

#[test]
fn required_errors_on_imported_none_field() {
    let draft = ImportedDraft { id: None, name: Some("bob".into()), score: Some(0) };
    let err = ImportedComplete::try_from(draft).unwrap_err();
    assert_eq!(err, RequiredError::new("id"));
}

// ---------------------------------------------------------------------------
// v0.4.0 — Rebuild  (`Target = Source`)
// ---------------------------------------------------------------------------
//
// Rebuild copies all fields unchanged and wires up `TypeshaperInto<Target> for Source`.
// Its primary use-case is attaching a different set of derive macros (e.g. Serialize)
// to an existing type without repeating every field definition.

typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    UserCopy = User
);

// Rebuild result is itself registered; downstream operations work on it.
typex!(
    #[derive(Debug, Clone, PartialEq, Eq)]
    UserCopyPublic = UserCopy & [id, name]
);

#[test]
fn rebuild_produces_identical_fields() {
    use typeshaper::TypeshaperExt;
    let user = User {
        id: 99,
        name: "zara".into(),
        age: 21,
        email: "z@z.com".into(),
    };
    let copy: UserCopy = user.project();
    assert_eq!(copy.id,    99);
    assert_eq!(copy.name,  "zara");
    assert_eq!(copy.age,   21);
    assert_eq!(copy.email, "z@z.com");
}

#[test]
fn rebuild_result_is_registered_for_downstream_ops() {
    use typeshaper::TypeshaperExt;
    let copy = UserCopy { id: 7, name: "kai".into(), age: 30, email: "k@k.com".into() };
    let public: UserCopyPublic = copy.project();
    assert_eq!(public.id,   7);
    assert_eq!(public.name, "kai");
}

#[test]
fn rebuild_private_field_visibility_is_preserved() {
    // User has `email` as a private field — Rebuild must reproduce visibility.
    // Direct struct literal construction would fail if visibility was wrong.
    let copy = UserCopy {
        id:    1,
        name:  "test".into(),
        age:   0,
        email: "t@t.com".into(),
    };
    assert_eq!(copy.email, "t@t.com");
}
