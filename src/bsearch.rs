// 二分探索の実装
// Rust標準ライブラリのslice::binary_search_byの改良版
use std::cmp::Ordering::{self, Greater, Less};

#[rustfmt::skip]
/*
 * このコードはRust標準ライブラリを元にしています：
 * https://github.com/rust-lang/rust/blob/b01026de465d5a5ef51e32c1012c43927d2a111c/library/core/src/slice/mod.rs#L2186
 *
 * MIT License (以下略)
 * このライブラリを使用することで、カスタムのインデックスベース二分探索が可能
 */

/// インデックスベース二分探索アルゴリズム
///
/// 通常のslice::binary_search_byと異なり、配列の要素を直接受け取るのではなく、
/// インデックスに基づいて比較を行う関数を受け取る。これにより、より柔軟な
/// 探索が可能になる。
///
/// # Arguments
/// * `size` - 探索範囲のサイズ
/// * `f` - インデックスを受け取り、Orderingを返すクロージャ
///
/// # Returns
/// * Ok(index) - 要素が見つかった場合のインデックス
/// * Err(index) - 要素が見つからなかった場合の挿入位置
///
/// # Examples
/// ```
/// let a = vec![1, 2, 3, 5, 8, 13, 21];
/// let result = binary_search_by(a.len(), |idx| a[idx].cmp(&5));
/// assert_eq!(result, Ok(3));
/// ```
pub fn binary_search_by<F>(mut size: usize, mut f: F) -> Result<usize, usize>
where
    F: FnMut(usize) -> Ordering,
{
    // 左端と右端のインデックスを初期化
    let mut left = 0;
    let mut right = size;

    // 左端が右端より小さい間は探索を続ける
    while left < right {
        // 中央のインデックスを計算（オーバーフローを避けるため left + size/2 を使用）
        let mid = left + size / 2;

        // 中央の要素と目標値を比較
        let cmp = f(mid);

        if cmp == Less {
            // 中央の要素が目標値より小さい場合、左半分を探索範囲から除外
            left = mid + 1;
        } else if cmp == Greater {
            // 中央の要素が目標値より大きい場合、右半分を探索範囲から除外
            right = mid;
        } else {
            // 見つかった場合は該当インデックスを返す
            return Ok(mid);
        }

        // 次のイテレーションのために探索範囲のサイズを更新
        size = right - left;
    }

    // 見つからなかった場合は挿入位置を返す
    Err(left)
}

#[cfg(test)]
mod tests {
    use super::binary_search_by;

    /// 二分探索の基本動作をテストする
    ///
    /// フィボナッチ数列の一部を使って、正常ケースと境界ケースをテスト
    #[test]
    fn test() {
        let a = vec![1, 2, 3, 5, 8, 13, 21];

        // 存在する要素の探索
        assert_eq!(Ok(0), binary_search_by(a.len(), |idx| a[idx].cmp(&1)));
        assert_eq!(Ok(1), binary_search_by(a.len(), |idx| a[idx].cmp(&2)));
        assert_eq!(Ok(4), binary_search_by(a.len(), |idx| a[idx].cmp(&8)));
        assert_eq!(Ok(6), binary_search_by(a.len(), |idx| a[idx].cmp(&21)));

        // 存在しない要素の探索（挿入位置を確認）
        assert_eq!(Err(0), binary_search_by(a.len(), |idx| a[idx].cmp(&0))); // 最初より小さい
        assert_eq!(Err(4), binary_search_by(a.len(), |idx| a[idx].cmp(&6))); // 中間に挿入
        assert_eq!(Err(7), binary_search_by(a.len(), |idx| a[idx].cmp(&22))); // 最後より大きい
    }
}
