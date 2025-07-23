// memcmp比較可能エンコーディング
// バイト配列を辞書順でソート可能な形式にエンコード/デコードする
use std::cmp;

/// エスケープブロックの長さ（8バイトのデータ + 1バイトの長さ情報）
/// この値は可変長データを固定長ブロックに分割するために使用される
const ESCAPE_LENGTH: usize = 9;

/// 元のデータサイズからエンコード後のサイズを計算する
///
/// 可変長データを固定長ブロックに分割してエンコードするため、
/// 元のサイズより大きくなる。各8バイトのデータに対して1バイトの
/// 長さ情報が追加される。
///
/// # Arguments
/// * `len` - 元のデータの長さ
///
/// # Returns
/// エンコード後のデータサイズ
pub fn encoded_size(len: usize) -> usize {
    (len + (ESCAPE_LENGTH - 1)) / (ESCAPE_LENGTH - 1) * ESCAPE_LENGTH
}

/// バイト配列をmemcmp比較可能な形式にエンコードする
///
/// 可変長データを固定長ブロックに分割し、各ブロックの最後に長さ情報を付加する。
/// これにより、辞書順での比較が可能になる。B-Treeなどでキーの比較に使用される。
///
/// エンコード形式：
/// - 8バイトずつのデータブロック + 1バイトの長さ情報
/// - 最後のブロックは8バイト未満の場合、ゼロパディング
/// - 長さ情報は実際のデータ長（最後のブロック以外は9）
///
/// # Arguments
/// * `src` - エンコードする元データ
/// * `dst` - エンコード結果を格納するベクタ
pub fn encode(mut src: &[u8], dst: &mut Vec<u8>) {
    loop {
        // 現在のブロックでコピーするデータ長を決定（最大8バイト）
        let copy_len = cmp::min(ESCAPE_LENGTH - 1, src.len());

        // データをコピー
        dst.extend_from_slice(&src[0..copy_len]);
        src = &src[copy_len..];

        if src.is_empty() {
            // 最後のブロックの場合
            let pad_size = ESCAPE_LENGTH - 1 - copy_len;
            if pad_size > 0 {
                // 8バイトに満たない場合はゼロパディング
                dst.resize(dst.len() + pad_size, 0);
            }
            // 実際のデータ長を記録（最後のブロックマーカー）
            dst.push(copy_len as u8);
            break;
        }
        // 継続ブロックマーカー（フルサイズの9を記録）
        dst.push(ESCAPE_LENGTH as u8);
    }
}

/// エンコードされたデータを元の形式にデコードする
///
/// encode関数でエンコードされたデータを元のバイト配列に復元する。
/// 各ブロックの最後の長さ情報を読み取り、実際のデータ長を判定する。
///
/// # Arguments
/// * `src` - デコードするエンコード済みデータ（可変参照で残りデータを更新）
/// * `dst` - デコード結果を格納するベクタ
pub fn decode(src: &mut &[u8], dst: &mut Vec<u8>) {
    loop {
        // ブロックの最後の長さ情報を取得
        let extra = src[ESCAPE_LENGTH - 1];

        // 実際のデータ長を計算（最大8バイト）
        let len = cmp::min(ESCAPE_LENGTH - 1, extra as usize);

        // データを抽出してdstに追加
        dst.extend_from_slice(&src[..len]);

        // 次のブロックに進む
        *src = &src[ESCAPE_LENGTH..];

        // 最後のブロックかどうかを判定
        if extra < ESCAPE_LENGTH as u8 {
            // 長さ情報が9未満の場合は最後のブロック
            break;
        }
        // 長さ情報が9の場合は継続ブロック
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// エンコード・デコードの往復テスト
    ///
    /// 異なる長さの文字列データをエンコードしてから
    /// デコードし、元のデータが正しく復元されることを確認
    #[test]
    fn test() {
        let org1 = b"helloworld!memcmpable";
        let org2 = b"foobarbazhogehuga";

        // 2つの文字列を連続してエンコード
        let mut enc = vec![];
        encode(org1, &mut enc);
        encode(org2, &mut enc);

        // エンコードされたデータから順次デコード
        let mut rest = &enc[..];

        // 1つ目のデータをデコード
        let mut dec1 = vec![];
        decode(&mut rest, &mut dec1);
        assert_eq!(org1, dec1.as_slice());

        // 2つ目のデータをデコード
        let mut dec2 = vec![];
        decode(&mut rest, &mut dec2);
        assert_eq!(org2, dec2.as_slice());
    }
}
