// クエリ実行エンジン
//
// SQLクエリに相当する実行計画を構築・実行するためのフレームワーク。
// イテレータパターンとビジターパターンを組み合わせて、
// 効率的なクエリ実行を提供します。

use anyhow::Result;

use crate::btree::{self, BTree, SearchMode};
use crate::buffer::BufferPoolManager;
use crate::disk::PageId;
use crate::tuple;

/// タプル（レコード）の型エイリアス
/// 各カラムがVec<u8>として表現される
pub type Tuple = Vec<Vec<u8>>;

/// タプルスライスの型エイリアス（読み取り専用参照）
pub type TupleSlice<'a> = &'a [Vec<u8>];

/// タプルレベルの検索モード
///
/// B+Tree検索のTupleSearchModeをより高レベルな
/// タプル操作用に拡張したもの。キーの複数要素を
/// 組み合わせた検索条件を表現する。
pub enum TupleSearchMode<'a> {
    /// 先頭から検索開始
    Start,

    /// 指定されたキー値から検索開始
    /// 複数要素のキーに対応（複合キー）
    Key(&'a [&'a [u8]]),
}

impl<'a> TupleSearchMode<'a> {
    /// TupleSearchModeをB+Tree用のSearchModeに変換する
    ///
    /// # Returns
    /// B+Tree検索で使用するSearchMode
    fn encode(&self) -> SearchMode {
        match self {
            TupleSearchMode::Start => SearchMode::Start,
            TupleSearchMode::Key(tuple) => {
                let mut key = vec![];
                // 複数要素をmemcmpableエンコーディングで連結
                tuple::encode(tuple.iter(), &mut key);
                SearchMode::Key(key)
            }
        }
    }
}

/// クエリ実行エンジンのエグゼキュータートレイト
///
/// イテレータパターンを実装し、タプルを一つずつ返す。
/// 各実行ノード（スキャン、フィルタ、結合など）はこのトレイトを実装。
pub trait Executor {
    /// 次のタプルを取得する
    ///
    /// # Arguments
    /// * `bufmgr` - バッファプールマネージャー
    ///
    /// # Returns
    /// 次のタプル、または終了時はNone
    fn next(&mut self, bufmgr: &mut BufferPoolManager) -> Result<Option<Tuple>>;
}

/// ボックス化されたエグゼキュータ（トレイトオブジェクト）
pub type BoxExecutor<'a> = Box<dyn Executor + 'a>;

/// 実行計画ノードのトレイト
///
/// クエリ最適化の結果として生成される実行計画の各ノードを表現。
/// start()メソッドで実際の実行を開始し、エグゼキュータを返す。
pub trait PlanNode {
    /// 実行計画ノードを開始してエグゼキュータを返す
    ///
    /// # Arguments
    /// * `bufmgr` - バッファプールマネージャー
    ///
    /// # Returns
    /// 実行可能なエグゼキュータ
    fn start(&self, bufmgr: &mut BufferPoolManager) -> Result<BoxExecutor>;
}

/// 順次スキャン（SeqScan）実行計画ノード
///
/// テーブル全体またはキー範囲を順次スキャンする。
/// 条件を満たすタプルのみを返すフィルタ機能も含む。
pub struct SeqScan<'a> {
    /// スキャン対象テーブルのメタページID
    pub table_meta_page_id: PageId,

    /// 検索開始位置（Start または Key指定）
    pub search_mode: TupleSearchMode<'a>,

    /// スキャン継続条件（falseになるまでスキャン継続）
    pub while_cond: &'a dyn Fn(TupleSlice) -> bool,
}

impl<'a> PlanNode for SeqScan<'a> {
    /// SeqScan実行計画を開始する
    ///
    /// B+Treeイテレータを作成し、ExecSeqScanエグゼキュータでラップする。
    fn start(&self, bufmgr: &mut BufferPoolManager) -> Result<BoxExecutor> {
        let btree = BTree::new(self.table_meta_page_id);
        let table_iter = btree.search(bufmgr, self.search_mode.encode())?;
        Ok(Box::new(ExecSeqScan {
            table_iter,
            while_cond: self.while_cond,
        }))
    }
}

pub struct ExecSeqScan<'a> {
    table_iter: btree::Iter,
    while_cond: &'a dyn Fn(TupleSlice) -> bool,
}

impl<'a> Executor for ExecSeqScan<'a> {
    fn next(&mut self, bufmgr: &mut BufferPoolManager) -> Result<Option<Tuple>> {
        let (pkey_bytes, tuple_bytes) = match self.table_iter.next(bufmgr)? {
            Some(pair) => pair,
            None => return Ok(None),
        };
        let mut pkey = vec![];
        tuple::decode(&pkey_bytes, &mut pkey);
        if !(self.while_cond)(&pkey) {
            return Ok(None);
        }
        let mut tuple = pkey;
        tuple::decode(&tuple_bytes, &mut tuple);
        Ok(Some(tuple))
    }
}

pub struct Filter<'a> {
    pub inner_plan: &'a dyn PlanNode,
    pub cond: &'a dyn Fn(TupleSlice) -> bool,
}

impl<'a> PlanNode for Filter<'a> {
    fn start(&self, bufmgr: &mut BufferPoolManager) -> Result<BoxExecutor> {
        let inner_iter = self.inner_plan.start(bufmgr)?;
        Ok(Box::new(ExecFilter {
            inner_iter,
            cond: self.cond,
        }))
    }
}

pub struct ExecFilter<'a> {
    inner_iter: BoxExecutor<'a>,
    cond: &'a dyn Fn(TupleSlice) -> bool,
}

impl<'a> Executor for ExecFilter<'a> {
    fn next(&mut self, bufmgr: &mut BufferPoolManager) -> Result<Option<Tuple>> {
        loop {
            match self.inner_iter.next(bufmgr)? {
                Some(tuple) => {
                    if (self.cond)(&tuple) {
                        return Ok(Some(tuple));
                    }
                }
                None => return Ok(None),
            }
        }
    }
}

pub struct IndexScan<'a> {
    pub table_meta_page_id: PageId,
    pub index_meta_page_id: PageId,
    pub search_mode: TupleSearchMode<'a>,
    pub while_cond: &'a dyn Fn(TupleSlice) -> bool,
}

impl<'a> PlanNode for IndexScan<'a> {
    fn start(&self, bufmgr: &mut BufferPoolManager) -> Result<BoxExecutor> {
        let table_btree = BTree::new(self.table_meta_page_id);
        let index_btree = BTree::new(self.index_meta_page_id);
        let index_iter = index_btree.search(bufmgr, self.search_mode.encode())?;
        Ok(Box::new(ExecIndexScan {
            table_btree,
            index_iter,
            while_cond: self.while_cond,
        }))
    }
}

pub struct ExecIndexScan<'a> {
    table_btree: BTree,
    index_iter: btree::Iter,
    while_cond: &'a dyn Fn(TupleSlice) -> bool,
}

impl<'a> Executor for ExecIndexScan<'a> {
    fn next(&mut self, bufmgr: &mut BufferPoolManager) -> Result<Option<Tuple>> {
        let (skey_bytes, pkey_bytes) = match self.index_iter.next(bufmgr)? {
            Some(pair) => pair,
            None => return Ok(None),
        };
        let mut skey = vec![];
        tuple::decode(&skey_bytes, &mut skey);
        if !(self.while_cond)(&skey) {
            return Ok(None);
        }
        let mut table_iter = self
            .table_btree
            .search(bufmgr, SearchMode::Key(pkey_bytes))?;
        let (pkey_bytes, tuple_bytes) = table_iter.next(bufmgr)?.unwrap();
        let mut tuple = vec![];
        tuple::decode(&pkey_bytes, &mut tuple);
        tuple::decode(&tuple_bytes, &mut tuple);
        Ok(Some(tuple))
    }
}

pub struct IndexOnlyScan<'a> {
    pub index_meta_page_id: PageId,
    pub search_mode: TupleSearchMode<'a>,
    pub while_cond: &'a dyn Fn(TupleSlice) -> bool,
}

impl<'a> PlanNode for IndexOnlyScan<'a> {
    fn start(&self, bufmgr: &mut BufferPoolManager) -> Result<BoxExecutor> {
        let btree = BTree::new(self.index_meta_page_id);
        let index_iter = btree.search(bufmgr, self.search_mode.encode())?;
        Ok(Box::new(ExecIndexOnlyScan {
            index_iter,
            while_cond: self.while_cond,
        }))
    }
}

pub struct ExecIndexOnlyScan<'a> {
    index_iter: btree::Iter,
    while_cond: &'a dyn Fn(TupleSlice) -> bool,
}

impl<'a> Executor for ExecIndexOnlyScan<'a> {
    fn next(&mut self, bufmgr: &mut BufferPoolManager) -> Result<Option<Tuple>> {
        let (skey_bytes, pkey_bytes) = match self.index_iter.next(bufmgr)? {
            Some(pair) => pair,
            None => return Ok(None),
        };
        let mut skey = vec![];
        tuple::decode(&skey_bytes, &mut skey);
        if !(self.while_cond)(&skey) {
            return Ok(None);
        }
        let mut tuple = skey;
        tuple::decode(&pkey_bytes, &mut tuple);
        Ok(Some(tuple))
    }
}
