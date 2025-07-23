// バッファプール管理とページキャッシュシステム
//
// データベースのメモリ内ページキャッシュを管理します。
// ディスクI/Oを最小化するため、頻繁にアクセスされるページを
// メモリ内に保持し、Clock-Sweepアルゴリズムで効率的な
// ページ置換を実行します。

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io;
use std::ops::{Index, IndexMut};
use std::rc::Rc;

use crate::disk::{DiskManager, PageId, PAGE_SIZE};

/// バッファプール関連のエラー型
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// ディスクI/Oエラー
    #[error(transparent)]
    Io(#[from] io::Error),

    /// バッファプールに空きがない場合のエラー
    #[error("no free buffer available in buffer pool")]
    NoFreeBuffer,
}

/// バッファプール内のバッファを識別するID
///
/// バッファプール内の配列インデックスを型安全にラップする。
/// デバッグ出力やハッシュ計算に対応。
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash)]
pub struct BufferId(usize);

/// ページデータの型エイリアス（4KBの固定サイズ配列）
pub type Page = [u8; PAGE_SIZE];

/// メモリ内のページバッファを表す構造体
///
/// 単一のページデータとその状態（ページID、変更フラグ）を管理する。
/// RefCellとCellを使用して内部可変性を提供し、複数の参照から
/// 安全にアクセス可能にする。
#[derive(Debug)]
pub struct Buffer {
    /// このバッファが格納しているページのID
    pub page_id: PageId,

    /// ページデータ本体（4KB）
    /// RefCellにより実行時借用チェックで安全な可変アクセスを提供
    pub page: RefCell<Page>,

    /// ページが変更されているかのフラグ
    /// Cellにより複数の不変参照からでも変更可能
    pub is_dirty: Cell<bool>,
}

impl Default for Buffer {
    /// デフォルトのBufferを作成する
    ///
    /// 無効なページID、ゼロ埋めされたページデータ、
    /// 未変更フラグで初期化される。
    fn default() -> Self {
        Self {
            page_id: Default::default(),
            page: RefCell::new([0u8; PAGE_SIZE]),
            is_dirty: Cell::new(false),
        }
    }
}

/// バッファプール内の個々のフレーム
///
/// BufferとClock-Sweepアルゴリズム用の使用カウンタを組み合わせた構造。
/// 使用カウンタは最近のアクセス頻度を追跡し、置換候補の選択に使用される。
#[derive(Debug, Default)]
pub struct Frame {
    /// Clock-Sweepアルゴリズム用の使用カウンタ
    /// アクセス時にインクリメントされ、sweepで減少する
    usage_count: u64,

    /// 実際のバッファオブジェクト（参照カウンタ付き）
    buffer: Rc<Buffer>,
}

/// バッファプール本体
///
/// 固定サイズのフレーム配列とClock-Sweepアルゴリズムを実装。
/// ページの置換時はusage_countが最も低いフレームを選択する。
pub struct BufferPool {
    /// バッファフレームの配列
    buffers: Vec<Frame>,

    /// Clock-Sweepアルゴリズムの次の候補位置
    next_victim_id: BufferId,
}

impl BufferPool {
    /// 指定されたサイズのバッファプールを作成する
    ///
    /// # Arguments
    /// * `pool_size` - バッファプールのサイズ（フレーム数）
    ///
    /// # Returns
    /// 初期化されたBufferPoolインスタンス
    pub fn new(pool_size: usize) -> Self {
        let mut buffers = vec![];
        buffers.resize_with(pool_size, Default::default);
        let next_victim_id = BufferId::default();
        Self {
            buffers,
            next_victim_id,
        }
    }

    /// バッファプールのサイズを返す
    ///
    /// # Returns
    /// フレーム数
    fn size(&self) -> usize {
        self.buffers.len()
    }

    /// Clock-Sweepアルゴリズムで置換対象のバッファを選択する
    ///
    /// 使用カウンタが0のフレームを探し、見つからない場合は
    /// カウンタをデクリメントしながら次のフレームに進む。
    /// 全てのフレームが使用中（Rc::strong_count > 1）の場合は失敗。
    ///
    /// # Returns
    /// 置換可能なバッファのIDまたはNone（全てピン済みの場合）
    fn evict(&mut self) -> Option<BufferId> {
        let pool_size = self.size();
        let mut consecutive_pinned = 0;
        let victim_id = loop {
            let next_victim_id = self.next_victim_id;
            let frame = &mut self[next_victim_id];

            // 使用カウンタが0で、他から参照されていない場合は置換候補
            if frame.usage_count == 0 {
                break self.next_victim_id;
            }
            if Rc::get_mut(&mut frame.buffer).is_some() {
                frame.usage_count -= 1;
                consecutive_pinned = 0;
            } else {
                consecutive_pinned += 1;
                if consecutive_pinned >= pool_size {
                    return None;
                }
            }
            self.next_victim_id = self.increment_id(self.next_victim_id);
        };
        Some(victim_id)
    }

    fn increment_id(&self, buffer_id: BufferId) -> BufferId {
        BufferId((buffer_id.0 + 1) % self.size())
    }
}

impl Index<BufferId> for BufferPool {
    type Output = Frame;

    fn index(&self, index: BufferId) -> &Self::Output {
        &self.buffers[index.0]
    }
}

impl IndexMut<BufferId> for BufferPool {
    fn index_mut(&mut self, index: BufferId) -> &mut Self::Output {
        &mut self.buffers[index.0]
    }
}

pub struct BufferPoolManager {
    disk: DiskManager,
    pool: BufferPool,
    page_table: HashMap<PageId, BufferId>,
}

impl BufferPoolManager {
    pub fn new(disk: DiskManager, pool: BufferPool) -> Self {
        let page_table = HashMap::new();
        Self {
            disk,
            pool,
            page_table,
        }
    }

    pub fn fetch_page(&mut self, page_id: PageId) -> Result<Rc<Buffer>, Error> {
        if let Some(&buffer_id) = self.page_table.get(&page_id) {
            let frame = &mut self.pool[buffer_id];
            frame.usage_count += 1;
            return Ok(Rc::clone(&frame.buffer));
        }
        let buffer_id = self.pool.evict().ok_or(Error::NoFreeBuffer)?;
        let frame = &mut self.pool[buffer_id];
        let evict_page_id = frame.buffer.page_id;
        {
            let buffer = Rc::get_mut(&mut frame.buffer).unwrap();
            if buffer.is_dirty.get() {
                self.disk
                    .write_page_data(evict_page_id, buffer.page.get_mut())?;
            }
            buffer.page_id = page_id;
            buffer.is_dirty.set(false);
            self.disk.read_page_data(page_id, buffer.page.get_mut())?;
            frame.usage_count = 1;
        }
        let page = Rc::clone(&frame.buffer);
        self.page_table.remove(&evict_page_id);
        self.page_table.insert(page_id, buffer_id);
        Ok(page)
    }

    pub fn create_page(&mut self) -> Result<Rc<Buffer>, Error> {
        let buffer_id = self.pool.evict().ok_or(Error::NoFreeBuffer)?;
        let frame = &mut self.pool[buffer_id];
        let evict_page_id = frame.buffer.page_id;
        let page_id = {
            let buffer = Rc::get_mut(&mut frame.buffer).unwrap();
            if buffer.is_dirty.get() {
                self.disk
                    .write_page_data(evict_page_id, buffer.page.get_mut())?;
            }
            let page_id = self.disk.allocate_page();
            *buffer = Buffer::default();
            buffer.page_id = page_id;
            buffer.is_dirty.set(true);
            frame.usage_count = 1;
            page_id
        };
        let page = Rc::clone(&frame.buffer);
        self.page_table.remove(&evict_page_id);
        self.page_table.insert(page_id, buffer_id);
        Ok(page)
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        for (&page_id, &buffer_id) in self.page_table.iter() {
            let frame = &self.pool[buffer_id];
            let mut page = frame.buffer.page.borrow_mut();
            self.disk.write_page_data(page_id, page.as_mut())?;
            frame.buffer.is_dirty.set(false);
        }
        self.disk.sync()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempfile;

    #[test]
    fn test() {
        let mut hello = Vec::with_capacity(PAGE_SIZE);
        hello.extend_from_slice(b"hello");
        hello.resize(PAGE_SIZE, 0);
        let mut world = Vec::with_capacity(PAGE_SIZE);
        world.extend_from_slice(b"world");
        world.resize(PAGE_SIZE, 0);

        let disk = DiskManager::new(tempfile().unwrap()).unwrap();
        let pool = BufferPool::new(1);
        let mut bufmgr = BufferPoolManager::new(disk, pool);
        let page1_id = {
            let buffer = bufmgr.create_page().unwrap();
            assert!(bufmgr.create_page().is_err());
            let mut page = buffer.page.borrow_mut();
            page.copy_from_slice(&hello);
            buffer.is_dirty.set(true);
            buffer.page_id
        };
        {
            let buffer = bufmgr.fetch_page(page1_id).unwrap();
            let page = buffer.page.borrow();
            assert_eq!(&hello, page.as_ref());
        }
        let page2_id = {
            let buffer = bufmgr.create_page().unwrap();
            let mut page = buffer.page.borrow_mut();
            page.copy_from_slice(&world);
            buffer.is_dirty.set(true);
            buffer.page_id
        };
        {
            let buffer = bufmgr.fetch_page(page1_id).unwrap();
            let page = buffer.page.borrow();
            assert_eq!(&hello, page.as_ref());
        }
        {
            let buffer = bufmgr.fetch_page(page2_id).unwrap();
            let page = buffer.page.borrow();
            assert_eq!(&world, page.as_ref());
        }
    }
}
