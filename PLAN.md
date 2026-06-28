# DriftPatch 実装計画（egui版）

## Context

SI現場での複数要件による重複修正問題を解決するツール。
同一箇所を別要件で触らない前提のもと、lexerベースのdiffパッチを生成・適用することで、行番号が変わってもパッチを正確に当てられるようにする。
GUI必須（Windows、ノンエンジニアも使用）、複数人でパッチを作成するため命名衝突を防ぐ。

Node.js依存を排除するため、Tauri/Monaco/TypeScript構成から **純Rustのegui** 構成に変更。

---

## 技術スタック

| 用途 | 選択 | 理由 |
|---|---|---|
| GUI フレームワーク | **eframe + egui 0.31** | 純Rust、Node.js不要、Windowsネイティブバイナリ |
| コードエディタ | **egui_code_editor 0.3** | syntectベースのsyntax highlight、行番号付き |
| ファイルダイアログ | **rfd 0.15** | ネイティブファイル選択ダイアログ |
| Lexer (Rust) | **汎用正規表現ベース** | コメント・文字列・空白等の最小要素を識別。言語プロファイルで切り替え。ANTLR不要 |
| 文字コード判定 | **chardetng + encoding_rs** | Mozilla製。MS932/UTF-8の自動判定、Shift_JIS確実対応 |
| Token Diff | **similar crate** | Python difflib相当のLCS diff、実績あり |
| JSON | **serde + serde_json** | Rust標準的 |
| ユニークID | **uuid crate** | v4 UUID、命名衝突防止 |
| 設定永続化 | **serde_json + dirs crate** | AppDataにsettings.jsonを保存 |
| 配布 | `cargo build --release` → 単一exe | Node.js・WebView2不要、シンプルなexe配布 |

---

## プロジェクト構成

```
DriftPatch/
├── src/
│   ├── main.rs               # eframe::run_native エントリポイント
│   ├── app.rs                # DriftPatchApp 状態 + eframe::App impl
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── toolbar.rs        # ツールバー
│   │   ├── editors.rs        # 3列エディタレイアウト
│   │   ├── patch_panel.rs    # 下部パッチ一覧テーブル
│   │   └── settings_window.rs
│   ├── lexer/
│   │   ├── mod.rs
│   │   ├── token.rs
│   │   ├── profiles.rs
│   │   └── tokenizer.rs
│   ├── diff/
│   │   └── token_diff.rs
│   ├── patch/
│   │   ├── model.rs
│   │   ├── generator.rs
│   │   ├── applier.rs
│   │   ├── context.rs
│   │   ├── repository.rs
│   │   └── name_gen.rs
│   └── encoding/
│       └── detector.rs
├── Cargo.toml
└── PLAN.md
```
