# typeshaper

`typeshaper` lets you derive new struct types from existing ones in a single expression — omit fields, pick fields, merge two structs, make all fields optional, or restore them as required. Every generated type automatically receives conversion impls and can feed into further expressions.

[中文文档](docs/readme.zh.md)

## Have you ever written code like this?

```rust
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub active: bool,
    pub created_at: i64,
}
```

The API layer needs it, but `password_hash` must not be exposed, so you duplicate:

```rust
pub struct UserPublic {
    pub id: u64,
    pub name: String,
    pub email: String,
    // no password_hash
    pub role: String,
    pub active: bool,
    pub created_at: i64,
}

impl From<User> for UserPublic {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            name: u.name,
            email: u.email,
            role: u.role,
            active: u.active,
            created_at: u.created_at,
        }
    }
}
```

The search endpoint only needs `id` and `name`, so you duplicate again:

```rust
pub struct UserSummary {
    pub id: u64,
    pub name: String,
}
// ... another From ...
```

The patch endpoint requires all fields to be optional, so you duplicate once more:

```rust
pub struct UserPatch {
    pub id: Option<u64>,
    pub name: Option<String>,
    pub email: Option<String>,
    pub password_hash: Option<String>,
    // ...
}
// ... another From ...
```

Add one field to `User` and you must update `UserPublic`, `UserPatch`, `UserSummary` — structs, `From` impls, and every test you might have missed.

And that's just `User`. You still have `Order`, `Product`, `Article`, `Comment`…

---

## A different approach

```toml
[dependencies]
typeshaper = "0.1"
```

```rust
use typeshaper::{TypeshaperExt, typeshaper, typex};

#[typeshaper]
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
    pub password_hash: String,
    pub role: String,
    pub active: bool,
    pub created_at: i64,
}

// Remove two fields
typex!(#[derive(Debug, Clone)] UserPublic  = User - [password_hash, created_at]);

// Keep only two fields
typex!(#[derive(Debug, Clone)] UserSummary = User & [id, name]);

// Make all fields optional
typex!(#[derive(Debug, Clone)] UserPatch   = User?);
```

Conversions just work:

```rust
let user: User = /* from the database */;

let public:  UserPublic  = user.clone().project(); // drops password_hash, created_at
let summary: UserSummary = user.clone().project(); // only id and name
let patch    = UserPatch::from(user);              // all fields become Option
```

Add a field to `User` — the three `typex!()` lines stay unchanged, and the new field propagates automatically.

---

## Going further: merge two sources into one

An order snapshot needs both user and address information:

```rust
#[typeshaper]
pub struct Address {
    pub street: String,
    pub city: String,
    pub country: String,
}

// Merge User and Address into a new type
typex!(#[derive(Debug, Clone)] OrderSnapshot = User + Address);

let snapshot = OrderSnapshot::from((user, address));
```

Keep only the fields that are in `User` but not in `Address`:

```rust
typex!(#[derive(Debug, Clone)] UserOnly = User % Address); // Diff
```

---

## Expressions compose

```rust
// Remove password_hash, then make remaining fields optional
typex!(#[derive(Debug)] UserSafePatch = User - [password_hash]?);

// Remove password_hash, then pick summary fields
typex!(#[derive(Debug)] UserSafeDto = User - [password_hash] & [id, name, email]);

// Parentheses control associativity: Partial then Required (round-trip)
typex!(#[derive(Debug)] UserRestored = (User - [password_hash])?!);
```

---

## Full patch round-trip

```rust
// Optional version for update endpoints
typex!(#[derive(Debug, Clone)] UserPatch    = User?);

// Restore to required after validation
typex!(#[derive(Debug, Clone)] UserVerified = UserPatch!);

// ---

let patch = UserPatch {
    name: Some("alice".into()),
    email: Some("new@example.com".into()),
    // other fields left as None — "no update"
    ..Default::default()
};

// Recover the fully-typed version if all fields are present
match UserVerified::try_from(patch) {
    Ok(verified) => { /* commit */ }
    Err(e)       => { /* report which field is missing */ }
}
```

---

## Cross-crate: define models once

Define the domain model in one crate; derive views in another without copying structs:

```rust
// core-crate/src/lib.rs
#[typeshaper(export)]          // export generates a companion macro
#[derive(Debug, Clone)]
pub struct User { /* ... */ }
// automatically exports: pub macro typeshaper_import_User!()
```

```rust
// api-crate/src/lib.rs
use core_crate::{User, typeshaper_import_User};

typeshaper_import_User!();  // registers User's field metadata in this crate

// works exactly like a locally annotated type
typex!(#[derive(Debug, Clone)] UserPublic = User - [password_hash, created_at]);
typex!(#[derive(Debug, Clone)] UserPatch  = User?);
```

---

## Generic types

When a source struct has type parameters, you must declare them **explicitly** in `typex!()` — on the target name and on each generic source node. This is intentional: implicit inheritance would silently produce the wrong struct when multiple type parameters come from different sources.

### Basic type parameter

```rust
#[typeshaper]
#[derive(Debug, Clone)]
pub struct Wrapper<T> {
    pub inner: T,
    pub label: String,
    pub count: usize,
}

// <T> is declared on both the target name and the source node
typex!(#[derive(Debug, Clone)] WrapperNoLabel<T>  = Wrapper<T> - [label]);
typex!(#[derive(Debug, Clone)] WrapperPartial<T>  = Wrapper<T>?);
typex!(#[derive(Debug, Clone)] WrapperRequired<T> = WrapperPartial<T>!);

let w = Wrapper { inner: 42u32, label: "hi".into(), count: 3 };
let no_label: WrapperNoLabel<u32> = w.project();
```

### Multiple type parameters (Merge)

When merging two generic types, the target declares all parameters; each source node uses its own:

```rust
#[typeshaper]
pub struct Person<T> { pub name: T, pub age: u8 }

#[typeshaper]
pub struct Addr<U> { pub city: U, pub zip: String }

// T comes from Person, U comes from Addr — both declared on the target
typex!(#[derive(Debug)] PersonWithAddr<T, U> = Person<T> + Addr<U>);

let full = PersonWithAddr::from((person, addr));
```

### Inline trait bounds and where clauses

```rust
typex!(PrintableValue<T: std::fmt::Display + Clone> = Printable<T> - [note]);

typex!(ConstrainedData<T> where T: Clone + PartialEq = Constrained<T> - [meta]);
```

### Lifetime parameters

```rust
#[typeshaper]
pub struct Borrowed<'a> { pub name: &'a str, pub value: u32 }

typex!(BorrowedName<'a> = Borrowed<'a> & [name]);
```

### Cross-crate generic types

Generic parameter metadata is encoded in the companion macro and fully restored on import:

```rust
// core-crate
#[typeshaper(export)]
pub struct GenericModel<T> { pub id: u64, pub payload: T, pub hidden: bool }
```

```rust
// app-crate
typeshaper_import_GenericModel!();

typex!(#[derive(Debug)] ModelPublic<T> = GenericModel<T> - [hidden]);
typex!(#[derive(Debug)] ModelDraft<T>  = GenericModel<T>?);
```

> **Compile-error guard**: forgetting type parameters is caught at compile time:
> ```
> typex!(Bad = Wrapper - [label]);
> //          ^^^^^^^ error: type `Wrapper` has generic parameters;
> //                  declare them explicitly, e.g. `Target<T> = Wrapper<T>`
> ```

---

## Reference

### Installation

```toml
[dependencies]
typeshaper = "0.1"
```

### Source annotation: `#[typeshaper]`

Add once to a source struct. Field metadata is written to the compile-time registry; the struct itself is left unchanged.

| Form | Effect |
|------|--------|
| `#[typeshaper]` | Use within the same crate |
| `#[typeshaper(export)]` | Use within the same crate + generates `typeshaper_import_T!()` for other crates |

```rust
#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub age: u8,
    pub email: String,
}
```

`#[typeshaper]` stacks on top of any other attributes without affecting existing behavior.

---

### Operator reference

| Syntax | Name | Meaning | Generated impl |
|--------|------|---------|----------------|
| `T - [f1, f2]` | **Omit** | Remove listed fields | `TypeshaperInto<Target> for T` |
| `T & [f1, f2]` | **Pick** | Keep only listed fields | `TypeshaperInto<Target> for T` |
| `A + B` | **Merge** | Combine all fields of A and B (no duplicates) | `From<(A, B)> for Target` |
| `T?` | **Partial** | Wrap every field in `Option<_>` | `From<T> for Target` |
| `T!` | **Required** | Unwrap `Option<_>` from a Partial type | `TryFrom<T> for Target` (or `From<T>` when the source has no `Option` fields) |
| `A % B` | **Diff** | Fields present in A but absent in B (matched on both field name **and** type) | `TypeshaperInto<Target> for A` |

**Composition rules**

Operators are left-associative; parentheses change precedence:

```rust
// User - [age] & [id, name]  means  (User - [age]) & [id, name]
typex!(Dto      = User - [age] & [id, name]);

// Parentheses make the right side evaluate first
typex!(Full     = User + (Badge - [label]));

// Postfix chaining
typex!(Draft    = User - [password_hash]?);
typex!(Roundtrip = (User - [password_hash])?!);
```

---

### `typex!()` syntax

```
typex!( [#[attr...]]  TargetName[<Params>] [where ...]  =  Expr );
```

- **Attributes** (optional): placed before `TargetName`, forwarded verbatim to the generated struct; multiple attributes can be stacked. `typex!()` never adds any `#[derive]` on its own.
- **TargetName**: the name of the generated struct; also registered in the compile-time table so it can be used as a source in subsequent `typex!()` calls.
- **`<Params>`** (optional): explicit generic or lifetime parameters for the target type — required when any source in `Expr` is a generic type. Inline bounds (`T: Clone + Debug`) and separate `where` clauses are both accepted.
- **Expr**: a type-algebra expression — see the table above. Each source node that refers to a generic type must carry matching type arguments: `Source<T>`, `Source<'a>`, etc.

```rust
typex!(
    #[derive(Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    UserPublicDto = User & [id, name, email]
);
```

---

### Conversion methods

`TypeshaperExt` is blanket-implemented for all types; the target is inferred from the binding:

```rust
let public: UserPublic = user.project();   // equivalent to user.typeshaper_into()
```

`Merge` uses tuple `From`, `Partial` uses `From`, `Required` uses `TryFrom`:

```rust
let snapshot = OrderSnapshot::from((user, address));
let draft    = UserPatch::from(user);
let verified = UserVerified::try_from(draft)?;
```

---

### Cross-crate usage

**Exporting crate**

```rust
// core-crate/src/lib.rs
use typeshaper::typeshaper;

#[typeshaper(export)]
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub role: String,
    pub active: bool,
}
// automatically exports: pub macro typeshaper_import_User!()
```

**Importing crate**

```rust
// app-crate/src/lib.rs
use typeshaper::typex;
use core_crate::{User, typeshaper_import_User};

typeshaper_import_User!();  // call once at module top-level

typex!(#[derive(Debug, Clone)] UserPublic = User - [role, active]);
typex!(#[derive(Debug, Clone)] UserPatch  = User?);
```

Multiple types each get their own companion macro; cross-crate Merge and Diff are fully supported:

```rust
use core_crate::{Address, typeshaper_import_Address};

typeshaper_import_User!();
typeshaper_import_Address!();

typex!(#[derive(Debug, Clone)] OrderSnapshot = User + Address);
typex!(#[derive(Debug, Clone)] UserOnly      = User % Address);
```

| | Same crate | Cross-crate |
|---|---|---|
| Source annotation | `#[typeshaper]` | `#[typeshaper(export)]` |
| Caller prerequisite | none | `typeshaper_import_T!()` |
| `typex!()` syntax | identical | identical |

---

### Supported operations

- [x] Omit — `T - [fields]`
- [x] Pick — `T & [fields]`
- [x] Merge — `A + B`
- [x] Partial — `T?`
- [x] Required — `T!`
- [x] Diff — `A % B`
- [x] Expression composition and chaining
- [x] Attribute forwarding
- [x] Cross-crate export / import
- [x] Generics, lifetimes, and trait bounds — explicit type parameters required

---

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
