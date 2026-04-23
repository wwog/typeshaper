//! Visibility-control tests for `typex!`.
//!
//! Rule: no visibility keyword → private struct (accessible in same module).
//! Rule: explicit `pub` / `pub(crate)` / `pub(super)` → forwarded verbatim.

use typeshaper::{TypeshaperExt, typeshaper, typex};

// ---------------------------------------------------------------------------
// 1. Default (no vis) — private struct accessible in the same flat module
// ---------------------------------------------------------------------------

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct Base {
    pub x: u32,
    pub y: String,
    pub z: bool,
}

// No vis → private struct (Rust default)
typex!(#[derive(Debug, Clone, PartialEq)] NoVisOmit = Base - [z]);
typex!(#[derive(Debug, Clone, PartialEq)] NoVisPick = Base & [x]);

#[test]
fn default_vis_is_private_but_accessible_in_same_module() {
    let b = Base { x: 1, y: "a".into(), z: true };

    // Private structs are accessible within the same module.
    let omitted: NoVisOmit = b.clone().project();
    assert_eq!(omitted.x, 1);
    assert_eq!(omitted.y, "a");

    let picked: NoVisPick = b.project();
    assert_eq!(picked.x, 1);
}

// ---------------------------------------------------------------------------
// 2. Explicit `pub`
// ---------------------------------------------------------------------------

typex!(#[derive(Debug, Clone, PartialEq)] pub PubOmit = Base - [z]);

#[test]
fn explicit_pub_generates_pub_struct() {
    let b = Base { x: 2, y: "b".into(), z: false };
    let o: PubOmit = b.project();
    assert_eq!(o.x, 2);
}

// ---------------------------------------------------------------------------
// 3. Explicit `pub(crate)`
// ---------------------------------------------------------------------------

typex!(#[derive(Debug, Clone, PartialEq)] pub(crate) PubCrateOmit = Base - [z]);

#[test]
fn explicit_pub_crate_generates_pub_crate_struct() {
    let b = Base { x: 3, y: "c".into(), z: true };
    let o: PubCrateOmit = b.project();
    assert_eq!(o.x, 3);
}

// ---------------------------------------------------------------------------
// 4. Visibility forwarded through all operations
// ---------------------------------------------------------------------------

typex!(#[derive(Debug, Clone, PartialEq)] pub PubPick    = Base & [x, y]);
typex!(#[derive(Debug, Clone, PartialEq)] pub PubPartial = Base?);
typex!(#[derive(Debug, Clone, PartialEq)] pub PubDiff    = Base - [z]);

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct Extra { pub score: u32 }

typex!(#[derive(Debug, Clone, PartialEq)] pub PubMerge = Base + Extra);
typex!(#[derive(Debug, Clone, PartialEq)] pub PubRequired = PubPartial!);

#[test]
fn pub_pick_works() {
    let b = Base { x: 10, y: "d".into(), z: false };
    let p: PubPick = b.project();
    assert_eq!(p.x, 10);
    assert_eq!(p.y, "d");
}

#[test]
fn pub_partial_works() {
    let b = Base { x: 11, y: "e".into(), z: true };
    let p = PubPartial::from(b);
    assert_eq!(p.x, Some(11));
    assert_eq!(p.y, Some("e".into()));
    assert_eq!(p.z, Some(true));
}

#[test]
fn pub_merge_works() {
    let b = Base { x: 12, y: "f".into(), z: false };
    let e = Extra { score: 99 };
    let m = PubMerge::from((b, e));
    assert_eq!(m.x, 12);
    assert_eq!(m.score, 99);
}

#[test]
fn pub_required_works() {
    use std::convert::TryFrom;
    let partial = PubPartial { x: Some(13), y: Some("g".into()), z: Some(false) };
    let r = PubRequired::try_from(partial).unwrap();
    assert_eq!(r.x, 13);
}
