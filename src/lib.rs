// Rellyデータベースエンジンのメインライブラリ
//
// このクレートは、Rustでゼロから実装されたリレーショナルデータベースエンジンRellyの
// コア機能を提供します。主要なコンポーネントは以下の通りです：
//
// - disk: ディスクベースのページ管理とファイルI/O
// - buffer: メモリバッファプールとページキャッシュ
// - btree: B+Treeインデックス構造の実装
// - table: テーブル操作とレコード管理
// - query: クエリ実行エンジンと実行計画
// - tuple: タプル（レコード）のエンコーディング
// - slotted: スロットページの実装
// - bsearch: 二分探索アルゴリズム
// - memcmpable: ソート可能なバイナリエンコーディング

mod bsearch;           // 二分探索アルゴリズム（内部使用）
pub mod btree;         // B+Treeインデックス実装
pub mod buffer;        // バッファプール管理
pub mod disk;          // ディスク管理とページI/O
mod memcmpable;        // memcmp比較可能エンコーディング（内部使用）
pub mod query;         // クエリ実行エンジン
mod slotted;           // スロットページ実装（内部使用）
pub mod table;         // テーブル操作
pub mod tuple;         // タプルエンコーディング
