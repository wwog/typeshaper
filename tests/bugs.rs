//! 修复后的行为验证测试。
//!
//! 隐患 2（T? 双重包裹）和隐患 3（Diff 按名）的正确行为。

use std::convert::TryFrom;
use typeshaper::{RequiredError, typeshaper, typex};

// ===========================================================================
// 隐患 2 修复验证：T? 对已有 Option<_> 字段原样保留（不报错也不双重包裹）
// ===========================================================================

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct WithOptionFields {
    pub maybe_name: Option<String>, // 已经是 Option<_>
    pub count: u32,
}

// 修复后：编译通过，maybe_name 原样保留，count 被包裹
typex!(
    #[derive(Debug, Clone, PartialEq)]
    WithOptionFieldsPartial = WithOptionFields?
);

/// T? 对混合类型：已是 Option<_> 的字段原样，其余字段包裹
#[test]
fn issue2_partial_preserves_existing_option_fields() {
    let original = WithOptionFields {
        maybe_name: Some("alice".into()),
        count: 10,
    };
    let partial = WithOptionFieldsPartial::from(original);

    // maybe_name: Option<String> 原样保留（不变成 Option<Option<String>>）
    assert_eq!(partial.maybe_name, Some("alice".to_string()));
    // count: u32 被正常包裹成 Option<u32>
    assert_eq!(partial.count, Some(10));
}

/// T? 对 None 字段：None 保持 None（不变成 Some(None)）
#[test]
fn issue2_partial_none_stays_none() {
    let original = WithOptionFields { maybe_name: None, count: 0 };
    let partial = WithOptionFieldsPartial::from(original);

    assert_eq!(partial.maybe_name, None); // None 保持 None
    assert_eq!(partial.count, Some(0));
}

/// T? 幂等性：对 Partial 派生类型再次应用 T? 等价于无操作
#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct Simple { pub x: u32, pub y: u32 }

typex!(#[derive(Debug, Clone, PartialEq)] SimpleDraft  = Simple?);
typex!(#[derive(Debug, Clone, PartialEq)] SimpleDraft2 = SimpleDraft?);  // 二次 T?

#[test]
fn issue2_partial_is_idempotent_on_partial_derived_type() {
    let draft = SimpleDraft { x: Some(1), y: Some(2) };
    // SimpleDraft2 和 SimpleDraft 结构相同：二次 T? 是幂等的
    let draft2 = SimpleDraft2::from(draft);
    assert_eq!(draft2.x, Some(1));
    assert_eq!(draft2.y, Some(2));
}

/// T! 对混合类型：Option<_> 字段展开，非 Option 字段原样
#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct MixedOptional {
    pub id: u64,              // 非 Option
    pub name: Option<String>, // Option
}

typex!(#[derive(Debug, Clone, PartialEq)] MixedRequired = MixedOptional!);

#[test]
fn issue2_required_unwraps_option_fields_keeps_plain_fields() {
    let mixed = MixedOptional { id: 42, name: Some("bob".into()) };
    let required = MixedRequired::try_from(mixed).expect("name is Some");

    assert_eq!(required.id,   42);    // u64 原样保留
    assert_eq!(required.name, "bob"); // Option<String> 被展开
}

#[test]
fn issue2_required_on_mixed_type_errors_when_option_field_is_none() {
    let mixed = MixedOptional { id: 99, name: None };
    let err = MixedRequired::try_from(mixed).unwrap_err();
    assert_eq!(err, RequiredError::new("name"));
}

// ===========================================================================
// 隐患 3 修复验证：Diff 现在按 name + type 双重比较
// ===========================================================================

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct LeftStruct {
    pub id: u64,
    pub name: String,
    pub score: u32,
}

#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct RightStruct {
    pub id: String, // 同名，但类型与 LeftStruct.id 不同
    pub extra: bool,
}

// LeftStruct.id: u64 ≠ RightStruct.id: String → id 不应被排除
typex!(
    #[derive(Debug, Clone, PartialEq)]
    DiffResult = LeftStruct % RightStruct
);

/// 同名但类型不同的字段被保留
#[test]
fn issue3_diff_keeps_field_when_types_differ() {
    let result = DiffResult { id: 99, name: "alice".into(), score: 42 };
    assert_eq!(result.id,    99);
    assert_eq!(result.name,  "alice");
    assert_eq!(result.score, 42);
}

/// 同名且同类型的字段仍然被排除（基本 Diff 语义不变）
#[typeshaper]
#[derive(Debug, Clone, PartialEq)]
pub struct RightSameType {
    pub id: u64, // 与 LeftStruct.id 同名同类型
}

typex!(
    #[derive(Debug, Clone, PartialEq)]
    DiffSameType = LeftStruct % RightSameType
);

#[test]
fn issue3_diff_excludes_field_when_name_and_type_both_match() {
    // id: u64 == id: u64 → 排除；DiffSameType 只有 name 和 score
    let result = DiffSameType { name: "bob".into(), score: 7 };
    assert_eq!(result.name,  "bob");
    assert_eq!(result.score, 7);
}
