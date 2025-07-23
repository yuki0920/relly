// ファイル操作と型変換のための標準ライブラリのインポート
use std::convert::TryInto;
use std::fs::{File, OpenOptions};
use std::io::{self, prelude::*, SeekFrom};
use std::path::Path;

// ゼロコピーシリアライゼーション/デシリアライゼーションのための外部クレート
use zerocopy::{AsBytes, FromBytes};

/// データベースストレージシステムの標準ページサイズ（4KB）
/// これは一般的なOSページサイズと一致し、ディスクI/O操作に最適
pub const PAGE_SIZE: usize = 4096;

/// ページの一意識別子を表す構造体
///
/// データベースでは、データを固定サイズのページに分割して管理する。
/// PageIdは各ページを一意に識別するための識別子として機能する。
///
/// FromBytes/AsBytesトレイトにより、効率的なバイナリシリアライゼーションが可能
/// #[repr(C)]により、メモリレイアウトがC言語と互換性を持つ
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, FromBytes, AsBytes)]
#[repr(C)]
pub struct PageId(pub u64);
impl PageId {
    /// 無効なページIDを表す定数（u64の最大値を使用）
    /// データベースシステムでよく使われる番兵値パターン
    pub const INVALID_PAGE_ID: PageId = PageId(u64::MAX);

    /// PageIdが有効かどうかを判定する
    ///
    /// # Returns
    /// 有効な場合はSome(PageId)、無効（INVALID_PAGE_ID）な場合はNoneを返す
    pub fn valid(self) -> Option<PageId> {
        if self == Self::INVALID_PAGE_ID {
            None
        } else {
            Some(self)
        }
    }

    /// PageIdの内部値（u64）を取得する
    ///
    /// # Returns
    /// PageIdが保持するu64値
    pub fn to_u64(self) -> u64 {
        self.0
    }
}

/// PageIdのDefaultトレイト実装
/// デフォルト値として無効なページIDを返す
impl Default for PageId {
    fn default() -> Self {
        Self::INVALID_PAGE_ID
    }
}

/// Option<PageId>からPageIdへの変換を提供
/// Noneの場合はデフォルト値（INVALID_PAGE_ID）を使用
impl From<Option<PageId>> for PageId {
    fn from(page_id: Option<PageId>) -> Self {
        page_id.unwrap_or_default()
    }
}

/// バイト配列からPageIdへの変換を提供
/// ネイティブエンディアンでu64を読み取り、PageIdに変換
impl From<&[u8]> for PageId {
    fn from(bytes: &[u8]) -> Self {
        let arr = bytes.try_into().unwrap();
        PageId(u64::from_ne_bytes(arr))
    }
}

/// ディスク上のページを管理するメインクラス
///
/// データベースファイルの読み書きと、新しいページの割り当てを管理する。
/// ページベースの永続化ストレージシステムを提供する。
pub struct DiskManager {
    /// データベースファイルのハンドル（ヒープファイル）
    heap_file: File,
    /// 次に割り当てられるページのID（連続割り当て）
    next_page_id: u64,
}

impl DiskManager {
    /// 既存のファイルハンドルからDiskManagerを作成する
    ///
    /// ファイルサイズを調べて、既存のページ数を計算し、
    /// 次に割り当てるページIDを決定する。
    ///
    /// # Arguments
    /// * `heap_file` - データベースファイルのハンドル
    ///
    /// # Returns
    /// DiskManagerインスタンスまたはI/Oエラー
    pub fn new(heap_file: File) -> io::Result<Self> {
        let heap_file_size = heap_file.metadata()?.len();
        let next_page_id = heap_file_size / PAGE_SIZE as u64;
        Ok(Self {
            heap_file,
            next_page_id,
        })
    }

    /// ファイルパスからDiskManagerを作成する
    ///
    /// 指定されたパスでファイルを開き（存在しない場合は作成）、
    /// 読み書き可能なモードで初期化する。
    ///
    /// # Arguments
    /// * `heap_file_path` - データベースファイルのパス
    ///
    /// # Returns
    /// DiskManagerインスタンスまたはI/Oエラー
    pub fn open(heap_file_path: impl AsRef<Path>) -> io::Result<Self> {
        let heap_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(heap_file_path)?;
        Self::new(heap_file)
    }

    /// 指定されたページからデータを読み取る
    ///
    /// ページIDを基にファイル内のオフセットを計算し、
    /// 該当位置からページサイズ分のデータを読み取る。
    ///
    /// # Arguments
    /// * `page_id` - 読み取り対象のページID
    /// * `data` - データを格納するバッファ（PAGE_SIZEと同じサイズであること）
    ///
    /// # Returns
    /// 成功時は()、失敗時はI/Oエラー
    pub fn read_page_data(&mut self, page_id: PageId, data: &mut [u8]) -> io::Result<()> {
        let offset = PAGE_SIZE as u64 * page_id.to_u64();
        self.heap_file.seek(SeekFrom::Start(offset))?;
        self.heap_file.read_exact(data)
    }

    /// 指定されたページにデータを書き込む
    ///
    /// ページIDを基にファイル内のオフセットを計算し、
    /// 該当位置にデータを書き込む。
    ///
    /// # Arguments
    /// * `page_id` - 書き込み対象のページID
    /// * `data` - 書き込むデータ（PAGE_SIZEと同じサイズであること）
    ///
    /// # Returns
    /// 成功時は()、失敗時はI/Oエラー
    pub fn write_page_data(&mut self, page_id: PageId, data: &[u8]) -> io::Result<()> {
        let offset = PAGE_SIZE as u64 * page_id.to_u64();
        self.heap_file.seek(SeekFrom::Start(offset))?;
        self.heap_file.write_all(data)
    }

    /// 新しいページを割り当てる
    ///
    /// 現在のnext_page_idを使用して新しいページIDを生成し、
    /// 次回のために内部カウンターをインクリメントする。
    ///
    /// # Returns
    /// 新しく割り当てられたPageId
    pub fn allocate_page(&mut self) -> PageId {
        let page_id = self.next_page_id;
        self.next_page_id += 1;
        PageId(page_id)
    }

    /// ファイルをディスクに同期する
    ///
    /// バッファに残っているデータをフラッシュし、
    /// OSレベルでファイルをディスクに同期する。
    ///
    /// # Returns
    /// 成功時は()、失敗時はI/Oエラー
    pub fn sync(&mut self) -> io::Result<()> {
        self.heap_file.flush()?;
        self.heap_file.sync_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    /// DiskManagerの基本機能をテストする
    ///
    /// テストの流れ：
    /// 1. 一時ファイルを作成してDiskManagerを初期化
    /// 2. "hello"と"world"の2つのページを作成・書き込み
    /// 3. DiskManagerを閉じる（ファイルの永続化を確認するため）
    /// 4. 同じファイルで新しいDiskManagerを作成
    /// 5. 書き込んだデータが正しく読み取れることを確認
    #[test]
    fn test() {
        // 一時ファイルを作成し、ファイルハンドルとパスを取得
        let (data_file, data_file_path) = NamedTempFile::new().unwrap().into_parts();
        let mut disk = DiskManager::new(data_file).unwrap();

        // "hello"データを含むページを作成
        let mut hello = Vec::with_capacity(PAGE_SIZE);
        hello.extend_from_slice(b"hello");
        hello.resize(PAGE_SIZE, 0); // 残りを0で埋める
        let hello_page_id = disk.allocate_page();
        disk.write_page_data(hello_page_id, &hello).unwrap();

        // "world"データを含むページを作成
        let mut world = Vec::with_capacity(PAGE_SIZE);
        world.extend_from_slice(b"world");
        world.resize(PAGE_SIZE, 0); // 残りを0で埋める
        let world_page_id = disk.allocate_page();
        disk.write_page_data(world_page_id, &world).unwrap();

        // DiskManagerを破棄（ファイルの永続化を確認するため）
        drop(disk);

        // 新しいDiskManagerで同じファイルを開く
        let mut disk2 = DiskManager::open(&data_file_path).unwrap();
        let mut buf = vec![0; PAGE_SIZE];

        // "hello"ページが正しく読み取れることを確認
        disk2.read_page_data(hello_page_id, &mut buf).unwrap();
        assert_eq!(hello, buf);

        // "world"ページが正しく読み取れることを確認
        disk2.read_page_data(world_page_id, &mut buf).unwrap();
        assert_eq!(world, buf);
    }
}
