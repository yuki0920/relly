// テーブル操作とレコード管理
//
// データベーステーブルとインデックスの作成・操作機能を提供します。
// シンプルテーブル（主キーのみ）と通常テーブル（ユニークインデックス付き）の
// 両方をサポートします。

use anyhow::Result;

use crate::btree::BTree;
use crate::buffer::BufferPoolManager;
use crate::disk::PageId;
use crate::tuple;

/// シンプルテーブル - 主キーのみを持つ基本的なテーブル
///
/// 単一のB+Treeをストレージとして使用し、レコードの挿入・検索機能を提供。
/// 主キーとその他のカラムを分けて管理し、効率的な範囲検索をサポート。
#[derive(Debug)]
pub struct SimpleTable {
    /// テーブルのメタデータが格納されているページID
    /// B+TreeのメタページのPageIdを保持
    pub meta_page_id: PageId,

    /// 主キーを構成する要素数
    /// レコードの先頭からこの数分の要素が主キーとして扱われる
    pub num_key_elems: usize,
}

impl SimpleTable {
    /// 新しいシンプルテーブルを作成する
    ///
    /// 内部的に新しいB+Treeを作成し、そのメタページIDを保存。
    /// テーブル作成後にレコードの挿入が可能になる。
    ///
    /// # Arguments
    /// * `bufmgr` - バッファプールマネージャー
    ///
    /// # Returns
    /// 成功時は()、失敗時はエラー
    pub fn create(&mut self, bufmgr: &mut BufferPoolManager) -> Result<()> {
        let btree = BTree::create(bufmgr)?;
        self.meta_page_id = btree.meta_page_id;
        Ok(())
    }

    /// レコードをテーブルに挿入する
    ///
    /// レコードを主キー部分と値部分に分割し、それぞれを
    /// エンコードしてB+Treeに格納する。
    ///
    /// # Arguments
    /// * `bufmgr` - バッファプールマネージャー
    /// * `record` - 挿入するレコード（スライス配列）
    ///
    /// # Returns
    /// 成功時は()、失敗時はエラー（重複キー等）
    ///
    /// # Examples
    /// ```
    /// // 主キーが1要素の場合
    /// table.insert(bufmgr, &[b"key1", b"value1", b"value2"])?;
    /// ```
    pub fn insert(&self, bufmgr: &mut BufferPoolManager, record: &[&[u8]]) -> Result<()> {
        let btree = BTree::new(self.meta_page_id);

        // 主キー部分をエンコード
        let mut key = vec![];
        tuple::encode(record[..self.num_key_elems].iter(), &mut key);

        // 値部分（非主キーカラム）をエンコード
        let mut value = vec![];
        tuple::encode(record[self.num_key_elems..].iter(), &mut value);

        // B+Treeに挿入
        btree.insert(bufmgr, &key, &value)?;
        Ok(())
    }
}

/// フルテーブル - 主キーとユニークインデックスを持つテーブル
///
/// 主テーブル用のB+Treeに加えて、複数のユニークインデックスを
/// サポートする拡張テーブル。各ユニークインデックスも独立した
/// B+Treeとして管理される。
#[derive(Debug)]
pub struct Table {
    /// 主テーブルのメタデータページID
    pub meta_page_id: PageId,

    /// 主キーを構成する要素数
    pub num_key_elems: usize,

    /// ユニークインデックスのリスト
    /// 各インデックスは独立したB+Treeとして管理される
    pub unique_indices: Vec<UniqueIndex>,
}

impl Table {
    /// 新しいテーブルを作成する
    ///
    /// 主テーブル用のB+Treeと、すべてのユニークインデックス用の
    /// B+Treeを作成する。各インデックスは独立して管理される。
    ///
    /// # Arguments
    /// * `bufmgr` - バッファプールマネージャー
    ///
    /// # Returns
    /// 成功時は()、失敗時はエラー
    pub fn create(&mut self, bufmgr: &mut BufferPoolManager) -> Result<()> {
        // 主テーブルのB+Treeを作成
        let btree = BTree::create(bufmgr)?;
        self.meta_page_id = btree.meta_page_id;

        // 各ユニークインデックスのB+Treeを作成
        for unique_index in &mut self.unique_indices {
            unique_index.create(bufmgr)?;
        }
        Ok(())
    }

    /// レコードをテーブルとすべてのインデックスに挿入する
    ///
    /// 主テーブルへの挿入後、すべてのユニークインデックスにも
    /// 対応するエントリを挿入する。いずれかで重複エラーが
    /// 発生した場合は挿入を中止する。
    ///
    /// # Arguments
    /// * `bufmgr` - バッファプールマネージャー
    /// * `record` - 挿入するレコード
    ///
    /// # Returns
    /// 成功時は()、失敗時はエラー（重複キーやインデックス制約違反等）
    pub fn insert(&self, bufmgr: &mut BufferPoolManager, record: &[&[u8]]) -> Result<()> {
        let btree = BTree::new(self.meta_page_id);

        // 主キーと値をエンコード
        let mut key = vec![];
        tuple::encode(record[..self.num_key_elems].iter(), &mut key);
        let mut value = vec![];
        tuple::encode(record[self.num_key_elems..].iter(), &mut value);

        // 主テーブルに挿入
        btree.insert(bufmgr, &key, &value)?;

        // すべてのユニークインデックスに挿入
        for unique_index in &self.unique_indices {
            unique_index.insert(bufmgr, &key, record)?;
        }
        Ok(())
    }
}

/// ユニークインデックス - 特定カラムの組み合わせに対するユニーク制約
///
/// 指定されたカラムの組み合わせをセカンダリキーとして、
/// 主キーへの参照を値として持つB+Treeインデックス。
/// ユニーク制約により、同じセカンダリキーの重複を防ぐ。
#[derive(Debug)]
pub struct UniqueIndex {
    /// インデックスのメタデータページID
    pub meta_page_id: PageId,

    /// セカンダリキーを構成するカラムのインデックス配列
    /// レコード内の位置を指定（例: [0, 2] なら1番目と3番目のカラム）
    pub skey: Vec<usize>,
}

impl UniqueIndex {
    /// 新しいユニークインデックスを作成する
    ///
    /// インデックス用のB+Treeを新規作成し、メタページIDを保存。
    /// インデックス作成後はレコード挿入時に自動的に更新される。
    ///
    /// # Arguments
    /// * `bufmgr` - バッファプールマネージャー
    ///
    /// # Returns
    /// 成功時は()、失敗時はエラー
    pub fn create(&mut self, bufmgr: &mut BufferPoolManager) -> Result<()> {
        let btree = BTree::create(bufmgr)?;
        self.meta_page_id = btree.meta_page_id;
        Ok(())
    }

    /// インデックスエントリを挿入する
    ///
    /// 指定されたカラムの組み合わせをセカンダリキーとして抽出し、
    /// 主キーを値として持つエントリをインデックスに挿入する。
    ///
    /// # Arguments
    /// * `bufmgr` - バッファプールマネージャー
    /// * `pkey` - 主キーのバイト配列
    /// * `record` - 全レコードデータ
    ///
    /// # Returns
    /// 成功時は()、失敗時はエラー（ユニーク制約違反等）
    ///
    /// # Examples
    /// ```
    /// // カラム [0, 2] でユニークインデックスを作成する場合
    /// // record[0]とrecord[2]の組み合わせがセカンダリキーになる
    /// unique_index.insert(bufmgr, primary_key, &record)?;
    /// ```
    pub fn insert(
        &self,
        bufmgr: &mut BufferPoolManager,
        pkey: &[u8],
        record: &[impl AsRef<[u8]>],
    ) -> Result<()> {
        let btree = BTree::new(self.meta_page_id);

        // セカンダリキーを構築（指定されたカラムの組み合わせ）
        let mut skey = vec![];
        tuple::encode(
            self.skey.iter().map(|&index| record[index].as_ref()),
            &mut skey,
        );

        // セカンダリキー -> 主キーのマッピングを挿入
        btree.insert(bufmgr, &skey, pkey)?;
        Ok(())
    }
}
