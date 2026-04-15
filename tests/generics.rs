//! TDD：泛型、生命周期、trait bound 支持（显式语法版）
//!
//! 用户必须在 `typex!` 中显式声明目标类型的泛型参数，
//! 并在源类型上注明使用的类型参数，不依赖自动继承。
//!
//! 格式：
//!   typex!(Target<T>          = Source<T> - [field])
//!   typex!(Target<T, U>       = Left<T> + Right<U>)
//!   typex!(Target<T: Bound>   = Source<T> - [field])
//!   typex!(Target<T> where T: Bound = Source<T> - [field])

use std::convert::TryFrom;
use typeshaper::{TypeshaperExt, typeshaper, typex};

// ─── 1. 单类型参数：显式 <T> ─────────────────────────────────────────────────

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct Wrapper<T> {
    pub inner: T,
    pub label: String,
    pub count: usize,
}

typex!(
    #[derive(Debug, Clone, PartialEq)]
    WrapperNoLabel<T> = Wrapper<T> - [label]
);

typex!(
    #[derive(Debug, Clone, PartialEq)]
    WrapperInner<T> = Wrapper<T> & [inner]
);

typex!(
    #[derive(Debug, Clone, PartialEq)]
    WrapperCopy<T> = Wrapper<T>
);

typex!(
    #[derive(Debug, Clone, PartialEq)]
    WrapperDraft<T> = Wrapper<T>?
);

typex!(
    #[derive(Debug, Clone, PartialEq)]
    WrapperComplete<T> = WrapperDraft<T>!
);

#[test]
fn explicit_generic_omit() {
    let w = Wrapper { inner: 42u32, label: "hi".into(), count: 3 };
    let no_label: WrapperNoLabel<u32> = w.project();
    assert_eq!(no_label.inner, 42u32);
    assert_eq!(no_label.count, 3);
}

#[test]
fn explicit_generic_pick() {
    let w = Wrapper { inner: "hello".to_string(), label: "lbl".into(), count: 1 };
    let core: WrapperInner<String> = w.project();
    assert_eq!(core.inner, "hello");
}

#[test]
fn explicit_generic_rebuild() {
    let w = Wrapper { inner: 99u8, label: "x".into(), count: 0 };
    let copy: WrapperCopy<u8> = w.project();
    assert_eq!(copy.inner, 99u8);
    assert_eq!(copy.label, "x");
    assert_eq!(copy.count, 0);
}

#[test]
fn explicit_generic_partial() {
    let w = Wrapper { inner: 7u64, label: "y".into(), count: 5 };
    let draft = WrapperDraft::from(w);
    assert_eq!(draft.inner, Some(7u64));
    assert_eq!(draft.label, Some("y".into()));
    assert_eq!(draft.count, Some(5usize));
}

#[test]
fn explicit_generic_required_ok() {
    let draft: WrapperDraft<u64> = WrapperDraft {
        inner: Some(7u64),
        label: Some("y".into()),
        count: Some(5),
    };
    let complete = WrapperComplete::try_from(draft).expect("all Some");
    assert_eq!(complete.inner, 7u64);
    assert_eq!(complete.label, "y");
    assert_eq!(complete.count, 5);
}

#[test]
fn explicit_generic_required_error() {
    let draft: WrapperDraft<u64> = WrapperDraft { inner: None, label: Some("y".into()), count: Some(5) };
    let err = WrapperComplete::try_from(draft).unwrap_err();
    assert_eq!(err.field, "inner");
}

// ─── 2. 多类型参数：显式 <A, B> ──────────────────────────────────────────────

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct Pair<A, B> {
    pub first: A,
    pub second: B,
    pub tag: String,
}

typex!(
    #[derive(Debug, Clone, PartialEq)]
    PairNoTag<A, B> = Pair<A, B> - [tag]
);

#[test]
fn explicit_multi_param_omit() {
    let p = Pair { first: 1u32, second: "two".to_string(), tag: "t".into() };
    let no_tag: PairNoTag<u32, String> = p.project();
    assert_eq!(no_tag.first, 1u32);
    assert_eq!(no_tag.second, "two");
}

// ─── 3. 显式 Merge：两个源类型各自携带独立的类型参数 ──────────────────────────

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct Addr<U> {
    pub city: U,
    pub zip: String,
}

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct Person<T> {
    pub name: T,
    pub age: u8,
}

typex!(
    #[derive(Debug, Clone, PartialEq)]
    PersonWithAddr<T, U> = Person<T> + Addr<U>
);

#[test]
fn explicit_merge_multi_source_generics() {
    let person = Person { name: "alice".to_string(), age: 30 };
    let addr = Addr { city: 1u32, zip: "10001".into() };
    let full = PersonWithAddr::from((person, addr));
    assert_eq!(full.name, "alice");
    assert_eq!(full.age, 30);
    assert_eq!(full.city, 1u32);
    assert_eq!(full.zip, "10001");
}

// ─── 4. 内联 trait bound：Target<T: Bound> ───────────────────────────────────

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct Printable<T: std::fmt::Display + Clone> {
    pub value: T,
    pub note: String,
}

typex!(
    #[derive(Debug, Clone, PartialEq)]
    PrintableValue<T: std::fmt::Display + Clone> = Printable<T> - [note]
);

#[test]
fn explicit_inline_bound_propagated() {
    let p = Printable { value: 3.14f64, note: "pi".into() };
    let v: PrintableValue<f64> = p.project();
    assert_eq!(v.value, 3.14f64);
}

// ─── 5. where 子句：Target<T> where T: Bound ─────────────────────────────────

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct Constrained<T>
where
    T: Clone + PartialEq,
{
    pub data: T,
    pub meta: String,
}

typex!(
    #[derive(Debug, Clone, PartialEq)]
    ConstrainedData<T> where T: Clone + PartialEq = Constrained<T> - [meta]
);

#[test]
fn explicit_where_clause_propagated() {
    let c = Constrained { data: vec![1u8, 2, 3], meta: "bytes".into() };
    let d: ConstrainedData<Vec<u8>> = c.project();
    assert_eq!(d.data, vec![1u8, 2, 3]);
}

// ─── 6. 生命周期参数：Target<'a> ─────────────────────────────────────────────

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct Borrowed<'a> {
    pub name: &'a str,
    pub value: u32,
}

typex!(
    #[derive(Debug, Clone, PartialEq)]
    BorrowedName<'a> = Borrowed<'a> & [name]
);

#[test]
fn explicit_lifetime_pick() {
    let b = Borrowed { name: "alice", value: 42 };
    let named: BorrowedName<'_> = b.project();
    assert_eq!(named.name, "alice");
}

// ─── 7. Diff：显式左侧泛型 ───────────────────────────────────────────────────

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope<T> {
    pub id: u32,
    pub payload: T,
    pub checksum: u64,
}

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct EnvelopeHeader {
    pub id: u32,
    pub checksum: u64,
}

typex!(
    #[derive(Debug, Clone, PartialEq)]
    EnvelopeBody<T> = Envelope<T> % EnvelopeHeader
);

#[test]
fn explicit_diff_retains_generic_field() {
    let env = Envelope { id: 1, payload: "data".to_string(), checksum: 0xdead };
    let body: EnvelopeBody<String> = env.project();
    assert_eq!(body.payload, "data");
}

// ─── 8. 跨 crate 泛型 export / import ────────────────────────────────────────
//
// 同文件模拟：#[typeshaper(export)] 生成伴生宏，
// 调用 typeshaper_import_GenericExport!() 后可在"消费侧"使用 typex!。

#[typeshaper(export)]
#[derive(Debug, Clone, PartialEq)]
pub struct GenericExport<T> {
    pub id: u64,
    pub payload: T,
    pub hidden: bool,
}

// 模拟跨 crate 调用伴生宏，重新注册类型（含泛型元数据）
typeshaper_import_GenericExport!();

typex!(
    #[derive(Debug, Clone, PartialEq)]
    GenericExportPublic<T> = GenericExport<T> - [hidden]
);

typex!(
    #[derive(Debug, Clone, PartialEq)]
    GenericExportDraft<T> = GenericExport<T>?
);

#[test]
fn cross_crate_generic_omit() {
    let g = GenericExport { id: 1, payload: "hello".to_string(), hidden: false };
    let public: GenericExportPublic<String> = g.project();
    assert_eq!(public.id, 1);
    assert_eq!(public.payload, "hello");
}

#[test]
fn cross_crate_generic_partial() {
    let g = GenericExport { id: 2, payload: 99u32, hidden: true };
    let draft = GenericExportDraft::from(g);
    assert_eq!(draft.id, Some(2));
    assert_eq!(draft.payload, Some(99u32));
    assert_eq!(draft.hidden, Some(true));
}
