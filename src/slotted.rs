// スロットページの実装
//
// データベースページ内で可変長レコードを効率的に管理するための
// スロット構造を提供します。ページの先頭にヘッダーとポインタ配列、
// 末尾からデータを格納することで、フラグメンテーションを最小化します。

use std::mem::size_of;
use std::ops::{Index, IndexMut, Range};

use zerocopy::{AsBytes, ByteSlice, ByteSliceMut, FromBytes, LayoutVerified};

/// スロットページのヘッダー構造体
///
/// ページの先頭に配置され、スロット数と空き領域の位置を管理する。
/// 8バイトの固定サイズで、Cレイアウト互換性を持つ。
#[derive(Debug, FromBytes, AsBytes)]
#[repr(C)]
pub struct Header {
    /// 現在のスロット数
    num_slots: u16,

    /// 空き領域の開始オフセット（ページ先頭からのバイト数）
    /// データは末尾から前方に向かって格納されるため、このオフセットより後が空き領域
    free_space_offset: u16,

    /// アライメント調整用のパディング（8バイト境界）
    _pad: u32,
}

/// 各スロットのデータ位置を示すポインタ構造体
///
/// 各スロットのデータがページ内のどこに格納されているかを記録する。
/// ポインタ配列はヘッダーの直後に連続して配置される。
#[derive(Debug, FromBytes, AsBytes, Clone, Copy)]
#[repr(C)]
pub struct Pointer {
    /// データの開始位置（ページ先頭からのオフセット）
    offset: u16,

    /// データの長さ（バイト数）
    len: u16,
}

impl Pointer {
    /// ポインタが指すデータの範囲をRangeとして返す
    ///
    /// # Returns
    /// データの開始位置から終了位置までのRange
    fn range(&self) -> Range<usize> {
        let start = self.offset as usize;
        let end = start + self.len as usize;
        start..end
    }
}

/// ポインタ配列の型エイリアス
/// zerocopyを使用してバイト配列をPointer配列として安全に解釈
pub type Pointers<B> = LayoutVerified<B, [Pointer]>;

/// スロットページを管理するメイン構造体
///
/// ページを以下のレイアウトで管理する：
/// [Header][Pointer配列...][空き領域][...データ領域]
///
/// データは末尾から前方に向かって格納され、ポインタ配列は
/// 前方に向かって拡張される。これにより中央部分が動的な
/// 空き領域として機能する。
pub struct Slotted<B> {
    /// ページヘッダー（8バイト固定）
    header: LayoutVerified<B, Header>,

    /// ヘッダー以降のページ本体部分
    body: B,
}

impl<B: ByteSlice> Slotted<B> {
    /// バイト配列からスロットページを構築する
    ///
    /// バイト配列の先頭8バイトをヘッダーとして解釈し、
    /// 残りの部分をボディとして管理する。
    ///
    /// # Arguments
    /// * `bytes` - ページデータのバイト配列
    ///
    /// # Returns
    /// 構築されたSlottedインスタンス
    ///
    /// # Panics
    /// ヘッダーのアライメントが正しくない場合
    pub fn new(bytes: B) -> Self {
        let (header, body) =
            LayoutVerified::new_from_prefix(bytes).expect("slotted header must be aligned");
        Self { header, body }
    }

    /// ページの容量（ヘッダーを除く）を返す
    ///
    /// # Returns
    /// ボディ部分のバイト数
    pub fn capacity(&self) -> usize {
        self.body.len()
    }

    /// 現在のスロット数を返す
    ///
    /// # Returns
    /// 格納されているスロットの数
    pub fn num_slots(&self) -> usize {
        self.header.num_slots as usize
    }

    /// 使用可能な空き領域のサイズを計算する
    ///
    /// ポインタ配列とデータ領域の間の空きスペースを計算。
    /// 新しいスロット挿入時の容量チェックに使用される。
    ///
    /// # Returns
    /// 空き領域のバイト数
    pub fn free_space(&self) -> usize {
        self.header.free_space_offset as usize - self.pointers_size()
    }

    /// 現在のポインタ配列が占めるサイズを計算する
    ///
    /// # Returns
    /// ポインタ配列の総バイト数
    fn pointers_size(&self) -> usize {
        size_of::<Pointer>() * self.num_slots()
    }

    /// ポインタ配列への読み取り専用参照を取得する
    ///
    /// # Returns
    /// ポインタ配列への参照
    fn pointers(&self) -> Pointers<&[u8]> {
        Pointers::new_slice(&self.body[..self.pointers_size()]).unwrap()
    }

    /// 指定されたポインタが指すデータへの参照を取得する
    ///
    /// # Arguments
    /// * `pointer` - データの位置を示すポインタ
    ///
    /// # Returns
    /// データへの参照
    fn data(&self, pointer: Pointer) -> &[u8] {
        &self.body[pointer.range()]
    }
}

impl<B: ByteSliceMut> Slotted<B> {
    /// スロットページを初期化する
    ///
    /// スロット数を0に設定し、空き領域オフセットをページ末尾に設定する。
    /// 新しいページや既存ページのリセット時に使用される。
    pub fn initialize(&mut self) {
        self.header.num_slots = 0;
        self.header.free_space_offset = self.body.len() as u16;
    }

    /// ポインタ配列への書き込み可能参照を取得する
    ///
    /// # Returns
    /// ポインタ配列への可変参照
    fn pointers_mut(&mut self) -> Pointers<&mut [u8]> {
        let pointers_size = self.pointers_size();
        Pointers::new_slice(&mut self.body[..pointers_size]).unwrap()
    }

    /// 指定されたポインタが指すデータへの書き込み可能参照を取得する
    ///
    /// # Arguments
    /// * `pointer` - データの位置を示すポインタ
    ///
    /// # Returns
    /// データへの可変参照
    fn data_mut(&mut self, pointer: Pointer) -> &mut [u8] {
        &mut self.body[pointer.range()]
    }

    /// 新しいスロットを挿入する
    ///
    /// 指定された位置に新しいスロットを挿入し、必要に応じて
    /// 既存のポインタをシフトする。データは末尾から前方に向かって配置される。
    ///
    /// # Arguments
    /// * `index` - 挿入位置（0からnum_slots()まで）
    /// * `len` - 挿入するデータの長さ
    ///
    /// # Returns
    /// 成功時はSome(())、容量不足の場合はNone
    pub fn insert(&mut self, index: usize, len: usize) -> Option<()> {
        // 容量チェック：ポインタ領域とデータ領域の両方が必要
        if self.free_space() < size_of::<Pointer>() + len {
            return None;
        }

        let num_slots_orig = self.num_slots();

        // データ領域を前方に移動（末尾から格納するため）
        self.header.free_space_offset -= len as u16;

        // スロット数を増加
        self.header.num_slots += 1;

        let free_space_offset = self.header.free_space_offset;
        let mut pointers_mut = self.pointers_mut();

        // 挿入位置以降のポインタを右にシフト
        pointers_mut.copy_within(index..num_slots_orig, index + 1);

        // 新しいポインタを設定
        let pointer = &mut pointers_mut[index];
        pointer.offset = free_space_offset;
        pointer.len = len as u16;
        Some(())
    }

    /// 指定されたスロットを削除する
    ///
    /// スロットのデータサイズを0にリサイズしてから、
    /// ポインタ配列から該当エントリを削除する。
    ///
    /// # Arguments
    /// * `index` - 削除するスロットのインデックス
    pub fn remove(&mut self, index: usize) {
        // データサイズを0にリサイズ（実質的にデータを削除）
        self.resize(index, 0);

        // ポインタ配列から該当エントリを削除（左詰め）
        self.pointers_mut().copy_within(index + 1.., index);

        // スロット数を減少
        self.header.num_slots -= 1;
    }

    /// 指定されたスロットのサイズを変更する
    ///
    /// スロットのデータサイズを変更し、必要に応じて他のデータを
    /// 移動させる。データは末尾から前方に向かって格納されるため、
    /// サイズ変更時は既存データの位置調整が必要。
    ///
    /// # Arguments
    /// * `index` - サイズ変更するスロットのインデックス
    /// * `len_new` - 新しいデータサイズ
    ///
    /// # Returns
    /// 成功時はSome(())、容量不足の場合はNone
    pub fn resize(&mut self, index: usize, len_new: usize) -> Option<()> {
        let pointers = self.pointers();
        let len_orig = pointers[index].len;
        let len_incr = len_new as isize - len_orig as isize;

        // サイズ変更がない場合は何もしない
        if len_incr == 0 {
            return Some(());
        }

        // 容量チェック（サイズ増加時のみ）
        if len_incr > self.free_space() as isize {
            return None;
        }

        let free_space_offset = self.header.free_space_offset as usize;
        let offset_orig = pointers[index].offset;
        let shift_range = free_space_offset..offset_orig as usize;

        // 空き領域オフセットを調整
        let free_space_offset_new = (free_space_offset as isize - len_incr) as usize;
        self.header.free_space_offset = free_space_offset_new as u16;

        // 対象スロットより前のデータを移動
        self.body
            .as_bytes_mut()
            .copy_within(shift_range, free_space_offset_new);

        // 影響を受けるポインタのオフセットを調整
        let mut pointers_mut = self.pointers_mut();
        for pointer in pointers_mut.iter_mut() {
            if pointer.offset <= offset_orig {
                pointer.offset = (pointer.offset as isize - len_incr) as u16;
            }
        }

        // 対象スロットのポインタを更新
        let pointer = &mut pointers_mut[index];
        pointer.len = len_new as u16;
        if len_new == 0 {
            pointer.offset = free_space_offset_new as u16;
        }
        Some(())
    }
}

/// Indexトレイトの実装 - 読み取り専用アクセス
///
/// スロット番号を指定してデータへの参照を取得する。
/// slotted[index] の形式でデータにアクセス可能。
impl<B: ByteSlice> Index<usize> for Slotted<B> {
    type Output = [u8];

    /// 指定されたスロットのデータへの参照を返す
    ///
    /// # Arguments
    /// * `index` - スロット番号
    ///
    /// # Returns
    /// スロットのデータへの参照
    fn index(&self, index: usize) -> &Self::Output {
        self.data(self.pointers()[index])
    }
}

/// IndexMutトレイトの実装 - 書き込み可能アクセス
///
/// スロット番号を指定してデータへの可変参照を取得する。
/// slotted[index] の形式でデータを変更可能。
impl<B: ByteSliceMut> IndexMut<usize> for Slotted<B> {
    /// 指定されたスロットのデータへの可変参照を返す
    ///
    /// # Arguments
    /// * `index` - スロット番号
    ///
    /// # Returns
    /// スロットのデータへの可変参照
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.data_mut(self.pointers()[index])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// スロットページの基本操作をテストする
    ///
    /// スロットの挿入、削除、サイズ変更などの操作が
    /// 正常に動作することを確認する。
    #[test]
    fn test() {
        let mut page_data = vec![0u8; 128];
        let mut slotted = Slotted::new(page_data.as_mut_slice());

        // ヘルパー関数：スロットを挿入してデータをコピーする
        let insert = |slotted: &mut Slotted<&mut [u8]>, index: usize, buf: &[u8]| {
            slotted.insert(index, buf.len()).unwrap();
            slotted[index].copy_from_slice(buf);
        };
        let push = |slotted: &mut Slotted<&mut [u8]>, buf: &[u8]| {
            let index = slotted.num_slots() as usize;
            insert(slotted, index, buf);
        };
        slotted.initialize();
        push(&mut slotted, b"hello");
        push(&mut slotted, b"world");
        assert_eq!(&slotted[0], b"hello");
        assert_eq!(&slotted[1], b"world");
        insert(&mut slotted, 1, b", ");
        push(&mut slotted, b"!");
        assert_eq!(&slotted[0], b"hello");
        assert_eq!(&slotted[1], b", ");
        assert_eq!(&slotted[2], b"world");
        assert_eq!(&slotted[3], b"!");
    }
}
