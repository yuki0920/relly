// タプル（レコード）のエンコーディング・デコーディング
//
// データベースのレコードを効率的にシリアライズ/デシリアライズするための
// ユーティリティ関数を提供します。memcmpableエンコーディングを使用して
// 辞書順での比較が可能な形式でデータを格納します。

use std::fmt::{self, Debug};

use crate::memcmpable;

/// 複数の要素を連結してバイト配列にエンコードする
///
/// イテレータから取得した各要素をmemcmpableエンコーディングで
/// 順次エンコードし、単一のバイト配列に連結する。
///
/// このエンコーディングにより、タプル全体で辞書順比較が可能になり、
/// B-Treeでのキー比較に適している。
///
/// # Arguments
/// * `elems` - エンコードする要素のイテレータ
/// * `bytes` - エンコード結果を格納するベクタ
///
/// # Examples
/// ```
/// let mut encoded = Vec::new();
/// encode([b"hello", b"world"].iter(), &mut encoded);
/// ```
pub fn encode(elems: impl Iterator<Item = impl AsRef<[u8]>>, bytes: &mut Vec<u8>) {
    elems.for_each(|elem| {
        let elem_bytes = elem.as_ref();
        // エンコード後のサイズを事前計算してメモリを予約
        let len = memcmpable::encoded_size(elem_bytes.len());
        bytes.reserve(len);
        // memcmpableエンコーディングで要素を追加
        memcmpable::encode(elem_bytes, bytes);
    });
}

/// エンコードされたバイト配列から複数の要素にデコードする
///
/// encode関数でエンコードされたバイト配列を解析し、
/// 元の要素列に分割してベクタに格納する。
///
/// # Arguments
/// * `bytes` - デコードするエンコード済みバイト配列
/// * `elems` - デコード結果の要素を格納するベクタ
///
/// # Examples
/// ```
/// let mut elements = Vec::new();
/// decode(&encoded_bytes, &mut elements);
/// ```
pub fn decode(bytes: &[u8], elems: &mut Vec<Vec<u8>>) {
    let mut rest = bytes;
    // バイト配列の終端まで順次デコード
    while !rest.is_empty() {
        let mut elem = vec![];
        // 次の要素をデコード（restも更新される）
        memcmpable::decode(&mut rest, &mut elem);
        elems.push(elem);
    }
}

/// タプル要素の美しい表示のためのラッパー構造体
///
/// デバッグ出力時にタプルの内容を読みやすい形式で表示する。
/// 各要素がUTF-8文字列として解釈可能な場合は文字列として表示し、
/// そうでない場合は16進数バイト列として表示する。
pub struct Pretty<'a, T>(pub &'a [T]);

impl<'a, T: AsRef<[u8]>> Debug for Pretty<'a, T> {
    /// タプルの内容を読みやすい形式でフォーマットする
    ///
    /// 各要素について：
    /// - UTF-8として有効な場合: 文字列 + 16進バイト列
    /// - UTF-8として無効な場合: 16進バイト列のみ
    ///
    /// 出力例: Tuple("hello" [68 65 6c 6c 6f], [ff fe fd])
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_tuple("Tuple");
        for elem in self.0 {
            let bytes = elem.as_ref();
            match std::str::from_utf8(bytes) {
                Ok(s) => {
                    // UTF-8文字列として表示可能な場合
                    d.field(&format_args!("{:?} {:02x?}", s, bytes));
                }
                Err(_) => {
                    // バイナリデータの場合は16進数のみ
                    d.field(&format_args!("{:02x?}", bytes));
                }
            }
        }
        d.finish()
    }
}
