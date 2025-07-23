// B+Treeノードの基本構造
//
// B+Treeのリーフノードとブランチノードに共通する
// ヘッダー構造とノード種別の管理を提供します。

use zerocopy::{AsBytes, ByteSlice, ByteSliceMut, FromBytes, LayoutVerified};

use super::branch::Branch;
use super::leaf::Leaf;

/// リーフノードを表す識別子（8バイト固定）
pub const NODE_TYPE_LEAF: [u8; 8] = *b"LEAF    ";

/// ブランチノードを表す識別子（8バイト固定）
pub const NODE_TYPE_BRANCH: [u8; 8] = *b"BRANCH  ";

/// B+Treeノードのヘッダー構造体
///
/// 全てのノードの先頭に配置され、ノードの種類を識別する。
/// 8バイトの固定サイズで、文字列ベースの識別子を使用。
#[derive(Debug, FromBytes, AsBytes)]
#[repr(C)]
pub struct Header {
    /// ノードの種類（"LEAF    " または "BRANCH  "）
    pub node_type: [u8; 8],
}

/// B+Treeノードの汎用構造体
///
/// リーフノードとブランチノードの共通ヘッダーと
/// ノード固有のボディ部分を管理する。
pub struct Node<B> {
    /// ノード共通のヘッダー（8バイト）
    pub header: LayoutVerified<B, Header>,

    /// ヘッダー以降のノードボディ部分
    /// リーフまたはブランチの固有データが格納される
    pub body: B,
}

impl<B: ByteSlice> Node<B> {
    /// バイト配列からNodeを構築する
    ///
    /// # Arguments
    /// * `bytes` - ノードデータのバイト配列
    ///
    /// # Returns
    /// 構築されたNodeインスタンス
    ///
    /// # Panics
    /// ヘッダーのアライメントが正しくない場合
    pub fn new(bytes: B) -> Self {
        let (header, body) = LayoutVerified::new_from_prefix(bytes).expect("node must be aligned");
        Self { header, body }
    }
}

impl<B: ByteSliceMut> Node<B> {
    /// ノードをリーフノードとして初期化する
    ///
    /// ヘッダーのnode_typeフィールドにリーフ識別子を設定。
    /// ボディ部分の初期化は呼び出し側で行う必要がある。
    pub fn initialize_as_leaf(&mut self) {
        self.header.node_type = NODE_TYPE_LEAF;
    }

    /// ノードをブランチノードとして初期化する
    ///
    /// ヘッダーのnode_typeフィールドにブランチ識別子を設定。
    /// ボディ部分の初期化は呼び出し側で行う必要がある。
    pub fn initialize_as_branch(&mut self) {
        self.header.node_type = NODE_TYPE_BRANCH;
    }
}

/// ノードボディの種類を表すenum
///
/// ノードタイプに応じて、適切なリーフまたはブランチ構造体で
/// ボディ部分をラップする。パターンマッチングにより
/// 型安全なノード操作を提供。
pub enum Body<B> {
    /// リーフノードのボディ
    Leaf(Leaf<B>),

    /// ブランチノードのボディ
    Branch(Branch<B>),
}

impl<B: ByteSlice> Body<B> {
    /// ノードタイプとバイト配列からBodyを構築する
    ///
    /// # Arguments
    /// * `node_type` - ノードタイプ識別子
    /// * `bytes` - ボディ部分のバイト配列
    ///
    /// # Returns
    /// 適切な種類のBodyインスタンス
    ///
    /// # Panics
    /// 不正なnode_typeが指定された場合
    pub fn new(node_type: [u8; 8], bytes: B) -> Body<B> {
        match node_type {
            NODE_TYPE_LEAF => Body::Leaf(Leaf::new(bytes)),
            NODE_TYPE_BRANCH => Body::Branch(Branch::new(bytes)),
            _ => unreachable!(),
        }
    }
}
