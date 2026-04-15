/// 编译失败测试 — 验证各修复点确实产生预期的编译错误。
///
/// 每个 ui/*.rs 文件对应一个应当编译失败的场景；
/// 对应的 ui/*.stderr 文件记录期望的错误信息（由 `TRYBUILD=overwrite cargo test` 自动生成）。
#[test]
fn compile_errors() {
    let t = trybuild::TestCases::new();
    // Pick 重复字段
    t.compile_fail("tests/ui/pick_duplicate_field.rs");
    // typeshaper 属性无效参数
    t.compile_fail("tests/ui/sculpt_invalid_attr.rs");
    // 泛型源类型未显式声明泛型参数
    t.compile_fail("tests/ui/generic_without_params.rs");
}
