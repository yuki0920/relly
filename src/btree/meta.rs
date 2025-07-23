// B+Treeメタデータページ
//
// B+Treeの構造情報（ルートページIDなど）を管理するメタデータページ。
// B+Tree全体の制御情報を永続化し、データベース再起動時の復元に使用。

use zerocopy::{AsBytes, ByteSlice, FromBytes, LayoutVerified};

use crate::disk::PageId;

/// B+Treeメタデータのヘッダー構造体
///
/// B+Treeの基本的な構造情報を格納する。
/// 現在はルートページIDのみを管理するが、将来的には
/// ツリーの高さ、ノード数などの統計情報も追加可能。
#[derive(Debug, FromBytes, AsBytes)]
#[repr(C)]
pub struct Header {
    /// B+TreeのルートノードのページID
    /// 検索・挿入操作の開始点として使用される
    pub root_page_id: PageId,
}

/// B+Treeメタデータページの管理構造体
///
/// メタデータページのヘッダー部分と未使用領域を管理する。
/// 将来的な拡張のため、ヘッダー以降の領域は保持される。
pub struct Meta<B> {
    /// メタデータヘッダー（ルートページIDなど）
    pub header: LayoutVerified<B, Header>,

    /// ヘッダー以降の未使用領域
    /// 将来の機能拡張用に予約されている
    _unused: B,
}

impl<B: ByteSlice> Meta<B> {
    /// バイト配列からMetaを構築する
    ///
    /// # Arguments
    /// * `bytes` - メタデータページのバイト配列
    ///
    /// # Returns
    /// 構築されたMetaインスタンス
    ///
    /// # Panics
    /// ヘッダーのアライメントが正しくない場合
    pub fn new(bytes: B) -> Self {
        let (header, _unused) =
            LayoutVerified::new_from_prefix(bytes).expect("meta page must be aligned");
        Self { header, _unused }
    }
}
