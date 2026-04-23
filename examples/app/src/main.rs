use typeshaper::typex;
use typeshaper_example_core::{Address, User, typeshaper_import_Address, typeshaper_import_User};

// ── 跨 crate 注册 ──────────────────────────────────────────────────────────
//
// 这两行是跨 crate 使用的核心：core 包通过 `#[typeshaper(export)]` 将字段元数据
// 编码进 `typeshaper_import_*!()` 伴生宏；app 包调用该宏，将字段信息写入本包的
// 编译期 HashMap，之后 `typex!()` 便可将 User / Address 当本地类型使用。

typeshaper_import_User!();
typeshaper_import_Address!();

// ── 类型变换 ───────────────────────────────────────────────────────────────

// Omit: 移除敏感字段，对外安全展示
typex!(#[derive(Debug, Clone, PartialEq)] pub UserPublic = User - [role, active]);

// Pick: 只保留摘要字段
typex!(#[derive(Debug, Clone, PartialEq)] pub UserSummary = User & [id, name]);

// Partial: 所有字段变 Option，用于增量更新场景
typex!(#[derive(Debug, Clone, PartialEq)] pub UserPatch = User?);

// Merge: 将用户与收货地址合并为订单快照
typex!(#[derive(Debug, Clone, PartialEq)] pub OrderSnapshot = User + Address);

// Diff: 保留 User 中 Address 没有的字段（即用户专属字段）
typex!(#[derive(Debug, Clone, PartialEq)] pub UserOnly = User % Address);

// ── 主函数 ────────────────────────────────────────────────────────────────

fn main() {
    use typeshaper::TypeshaperExt;

    let user = User {
        id: 1,
        name: "Alice".into(),
        email: "alice@example.com".into(),
        role: "admin".into(),
        active: true,
    };
    let addr = Address {
        street: "123 Main St".into(),
        city: "Springfield".into(),
        country: "US".into(),
    };

    let public: UserPublic = user.clone().project();
    println!("Public  : {:?}", public);

    let summary: UserSummary = user.clone().project();
    println!("Summary : {:?}", summary);

    let snapshot = OrderSnapshot::from((user.clone(), addr));
    println!("Snapshot: {:?}", snapshot);
}

// ── 测试 ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use typeshaper::TypeshaperExt;
    use std::convert::TryFrom;

    fn sample_user() -> User {
        User {
            id: 42,
            name: "Bob".into(),
            email: "bob@example.com".into(),
            role: "editor".into(),
            active: true,
        }
    }

    fn sample_addr() -> Address {
        Address {
            street: "456 Oak Ave".into(),
            city: "Shelbyville".into(),
            country: "US".into(),
        }
    }

    #[test]
    fn omit_removes_role_and_active() {
        let public: UserPublic = sample_user().project();
        assert_eq!(public.id,    42);
        assert_eq!(public.name,  "Bob");
        assert_eq!(public.email, "bob@example.com");
    }

    #[test]
    fn pick_keeps_only_id_and_name() {
        let summary: UserSummary = sample_user().project();
        assert_eq!(summary.id,   42);
        assert_eq!(summary.name, "Bob");
    }

    #[test]
    fn partial_wraps_all_fields_in_option() {
        let patch = UserPatch::from(sample_user());
        assert_eq!(patch.id,     Some(42));
        assert_eq!(patch.name,   Some("Bob".into()));
        assert_eq!(patch.email,  Some("bob@example.com".into()));
        assert_eq!(patch.role,   Some("editor".into()));
        assert_eq!(patch.active, Some(true));
    }

    #[test]
    fn required_restores_partial() {
        use typeshaper::typex;
        typex!(#[derive(Debug, Clone, PartialEq)] UserRestored = UserPatch!);

        let patch = UserPatch {
            id:     Some(1),
            name:   Some("Carol".into()),
            email:  Some("c@example.com".into()),
            role:   Some("viewer".into()),
            active: Some(false),
        };
        let restored = UserRestored::try_from(patch).unwrap();
        assert_eq!(restored.id,    1);
        assert_eq!(restored.name,  "Carol");
        assert_eq!(restored.email, "c@example.com");
        assert_eq!(restored.role,  "viewer");
        assert!(!restored.active);
    }

    #[test]
    fn merge_combines_user_and_address() {
        let snapshot = OrderSnapshot::from((sample_user(), sample_addr()));
        assert_eq!(snapshot.id,      42);
        assert_eq!(snapshot.name,    "Bob");
        assert_eq!(snapshot.city,    "Shelbyville");
        assert_eq!(snapshot.country, "US");
    }

    #[test]
    fn diff_keeps_user_only_fields() {
        // User fields: id, name, email, role, active
        // Address fields: street, city, country
        // No overlap → UserOnly keeps all User fields
        let only: UserOnly = sample_user().project();
        assert_eq!(only.id,     42);
        assert_eq!(only.name,   "Bob");
        assert_eq!(only.email,  "bob@example.com");
        assert_eq!(only.role,   "editor");
        assert!(only.active);
    }
}
