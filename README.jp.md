# DriftPatch

**DriftPatch** は、ソースコードの変更をパッチ（`.dpatch`）として生成・管理・適用するデスクトップツールです。変更前後のテキストからトークン認識型の JSON パッチを作成し、パッチリポジトリに保存して、GUI または CLI で対象ファイルに適用できます。

> English documentation: [README.md](README.md)

## 特徴

- **GUI エディタ** — 3列レイアウト（修正前 / 編集画面 / プレビュー）
- **トークン認識型パッチ** — Java、Python、C/C++、SQL、JavaScript/TypeScript、Rust、C#、Go、PL/SQL および汎用プロファイルに対応
- **文字コード対応** — 読み込み時に自動検出、書き込み時は元のエンコーディングを維持
- **パッチリポジトリ** — `patches/<target_file>/` 配下に整理して保存
- **バッチ CLI** — 全パッチを一括適用し、Excel / HTML レポートを出力
- **Git コミット取り込み** — Git 履歴から指定コミットの変更を `.dpatch` として一括生成（読み取り専用、Git 操作は行わない）
- **Git 操作非依存** — commit/push 等の Git 操作は行わない。履歴参照には libgit2 を使用

## 必要環境

- [Rust](https://www.rust-lang.org/) ツールチェーン（edition 2021）
- eframe/egui が動作するデスクトップ環境（Windows、X11/Wayland 対応 Linux）
- Git コミット取り込み機能を使う場合: [CMake](https://cmake.org/)（libgit2 のビルドに必要）

Windows では、システムフォント（游ゴシック、MS ゴシック、メイリオ）から日本語フォントを自動読み込みします。

## ビルドと実行

```bash
# 両バイナリをビルド（release）
cargo build --release

# GUI
cargo run --release
# または
./target/release/driftpatch

# バッチ CLI
cargo run --release --bin driftpatch-batch -- apply \
  --workdir /path/to/project \
  --patch-dir /path/to/repo/patches \
  --report-dir /path/to/reports
```

## GUI の使い方

### 初回設定

1. `driftpatch` を起動する。
2. **設定**（歯車ボタン）をクリックする。
3. 以下を設定する:

| 設定項目 | 説明 |
|----------|------|
| **ユーザー名** | 生成するパッチに記録される作者名 |
| **パッチリポジトリパス** | パッチリポジトリのルート（`patches/` フォルダを含む） |
| **work ディレクトリ** | 対象ファイルの基準ディレクトリ。パッチ内のパスはここからの相対パス |
| **Git リポジトリパス** | Git コミット取り込み用。空の場合は work ディレクトリを使用 |

4. **保存して閉じる** をクリックする。

設定は次の場所に保存されます:

- **Windows:** `%APPDATA%\DriftPatch\settings.json`
- **Linux:** `~/.local/share/DriftPatch/settings.json`
- **macOS:** `~/Library/Application Support/DriftPatch/settings.json`

### 基本的なワークフロー

```mermaid
flowchart LR
    openFile[ファイルを開く] --> edit[中央列で編集]
    edit --> generate[パッチ生成]
    generate --> save[patches に保存]
    save --> select[一覧で選択]
    select --> preview[右列でプレビュー]
    preview --> apply[適用]
```

1. **ファイルを開く** — **ファイルを開く** をクリックし、`work_dir` 配下のソースファイルを選択する。
2. **編集** — 中央列（**修正画面**）でテキストを編集する。左列は修正前の原文、削除箇所は赤、追加箇所は緑でハイライトされる。
3. **パッチ生成** — **パッチ生成...** をクリックし、説明（要件番号など）を入力して **生成** を押す。パッチは `patches/<target_file>/<id>.dpatch` に保存される。
4. **プレビュー** — 下部パネルでパッチを選択する。右列に、原文へ適用した結果が表示される。
5. **適用** — **適用** をクリックすると、選択したパッチがメモリ上の原文・編集テキストに反映される。
6. **削除** — **削除** をクリックすると、選択したパッチファイルがリポジトリから削除される。

### Git コミットからパッチ生成

1. **設定** で **Git リポジトリパス**（未設定時は work ディレクトリ）、**work ディレクトリ**、**パッチリポジトリパス** を設定する。
2. ツールバーの **Git コミットから取り込み** をクリックする。
3. コミット一覧から対象を選ぶか、SHA / ref を直接入力する。
4. 必要に応じて説明を上書きし、**生成** を押す。
5. コミット内の全変更ファイルについて `.dpatch` が生成される。同一ファイル内の複数箇所修正はハンク単位で別ファイルに分割される（`-h1`, `-h2` など）。

### 3列レイアウト

| 列 | ラベル | 用途 |
|----|--------|------|
| 左 | 修正前（読取専用） | 変更前のベースライン |
| 中央 | 修正画面（編集可） | パッチ作成用の作業コピー |
| 右 | パッチ適用プレビュー（読取専用） | 選択パッチの適用結果 |

左列・右列は中央列のスクロールに連動する。

### パッチ一覧パネル

下部パネルには、現在開いているファイルの `target_file` に一致するパッチのみ表示される。**更新** でディスクから再読み込みできる。

## バッチ CLI の使い方

`driftpatch-batch` は `--patch-dir` 以下の全 `.dpatch` を `--workdir` 内のファイルへ順次適用する。

```bash
driftpatch-batch apply \
  --workdir C:\project\src \
  --patch-dir C:\project\patch-repo\patches \
  --report-dir C:\project\reports
```

| オプション | 説明 |
|------------|------|
| `--workdir` | 修正対象ファイルを含むワークディレクトリ |
| `--patch-dir` | `.dpatch` が格納されたディレクトリ（通常は `patches/` またはリポジトリルート） |
| `--report-dir` | Excel / HTML レポートの出力先 |
| `--dry-run` | ファイルを一切変更せず、各パッチの予定操作（変更 / 作成 / 削除 / リネーム）と適用可否のみレポートする |

### レポート

実行後、`--report-dir` に次の2ファイルが生成される:

- `driftpatch-report-YYYYMMDD-HHMMSS.xlsx`
- `driftpatch-report-YYYYMMDD-HHMMSS.html`

各行にはパッチパス、対象ファイル、ステータス（`success` / `skipped` / `failed`）、エラー種別、タイムスタンプが記録される。`skipped` は既に適用済み（冪等検知）を意味し、失敗としては計上されない。

### 終了コード

| コード | 意味 |
|--------|------|
| `0` | 全パッチの適用に成功 |
| `1` | 1件以上のパッチが失敗、または致命的エラー |

失敗したパッチはレポートに記録され、残りのパッチ処理は継続される。

### パッチ競合の事前検査

`apply --dry-run` が「指定 work_dir に当たるか」の検査なのに対し、`check` は `--patch-dir` 内のパッチ同士が両立するかを work_dir 不要で検査する。

```bash
driftpatch-batch check --patch-dir C:\project\patch-repo\patches
```

検出対象:

| 検出内容 | 深刻度 | 説明 |
|----------|--------|------|
| 重複ハンク | 競合 (error) | 同一ファイルの同一トークン篇囲を触る2つのハンク |
| 削除対象への編集 | 競合 (error) | Delete 対象ファイルへの Modify / Rename-with-edit |
| リネーム旧パス宛パッチ | 警告 (warning) | Rename の `old_path` を target_file とする別パッチ（適用順序に依存） |

終了コード: 競合 (error) が1件でもあれば `1`、警告のみまたは問題なければ `0`。

`apply` は適用前にパッチ間の依存を解析し、`Create -> Modify -> Rename -> Delete` の基本優先度と Rename の旧パス・新パス依存で自動整列する。そのため「`Old.java` への Modify」と「`Old.java -> New.java` の Rename」が同じバッチにあっても、Modify を先に適用して失敗しない。

### 冪等な再適用

一部のパッチが既に適用済みの work_dir に対して `apply` を再実行しても安全である。対象ファイルの内容が Modify パッチの適用後内容と一致している（空白・インデント差は無視）場合、「適用済み」と検知され、`failed` でも `success` でもなく `skipped` ステータスとして報告される。スキップされたパッチはファイル書き込みも `.bak` バックアップ作成も行わない。`Create` と純 `Rename` パッチは以前から同等の冪等検知を持っていた。

この検知はヒューリスティックである点に注意。トークンを削除するだけのハンク（`added_text` が空）では、ハンクの前後コンテキストトークンが隣接しているかどうかで「適用済み」を判定する。ハンクが部分的にしか適用されていない場合（例: 期待マッチ数2件のうち1件のみ適用済み）はドリフトとして扱われ、スキップではなく `failed` として報告される。

### Git コミットからパッチ生成

```bash
driftpatch-batch from-commit \
  --repo C:\project \
  --commit abc1234 \
  --workdir C:\project \
  --patch-repo C:\project\patch-repo \
  --author alice \
  --description "REQ-123 fix null check" \
  --report-dir C:\project\reports
```

| オプション | 説明 |
|------------|------|
| `--repo` | Git リポジトリパス |
| `--commit` | コミット SHA または ref |
| `--workdir` | `target_file` 相対化の基準ディレクトリ |
| `--patch-repo` | 出力先パッチリポジトリルート（`patches/` の親） |
| `--author` | パッチ作者名（任意） |
| `--description` | パッチ説明（未指定時はコミットメッセージ） |
| `--report-dir` | レポート出力先（任意） |

同一ファイル内の複数箇所修正はハンク単位で別 `.dpatch` に分割される。

コミット内の全変更種別がパッチ化される: 変更は `modify`、新規追加は `create`、削除は `delete`（削除時点の significant token 列を `verify_tokens` として記録）、リネームは検出されて `rename` パッチになる。適用時、`delete` / 純粋な `rename` パッチは現物の内容が `verify_tokens` と一致する場合のみ削除・移動を行う（空白・インデント差は無視）。ドリフトしたファイルは変更されず、失敗としてレポートされる。

## パッチリポジトリの構成

```
patch-repo/
└── patches/
    └── src/
        └── Foo.java/
            ├── 20260628-fix-null-check-a1b2c3d4.dpatch
            └── 20260629-add-logging-e5f6g7h8.dpatch
```

- 各パッチは `patches/<target_file>/<filename>.dpatch` に配置される。
- `target_file` は `work_dir` からの相対パス（`/` 区切り。例: `src/Foo.java`）。
- ファイル名は `{YYYYMMDD}-{kebab-description}-{uuid8}.dpatch` 形式。
- 旧形式のフラット配置（`patches/*.dpatch` を直下に置く）も読み込み可能。

DriftPatch は **Git 操作（commit/push 等）を行いません**。コミット履歴からのパッチ生成には libgit2（`git2` crate）を読み取り専用で使用します。バージョン管理は利用者側で行ってください。

## `.dpatch` ファイル形式

`.dpatch` は JSON 形式。構造は以下のとおり。

### `PatchFile`（ルート）

| フィールド | 型 | 説明 |
|------------|-----|------|
| `version` | string | フォーマットバージョン（現在は `"1"`） |
| `id` | string | 一意のパッチ ID（`YYYYMMDD-kebab-uuid8`） |
| `author` | string | 設定のユーザー名 |
| `created_at` | string | 作成日時（ISO 8601） |
| `description` | string | 人間が読める説明 |
| `target_file` | string | `work_dir` からの相対パス（`/` 区切り） |
| `language` | string | 言語プロファイル名（例: `java`, `python`） |
| `encoding` | string | ファイルのエンコーディング（例: `UTF-8`） |
| `hunks` | array | 差分ハンクの配列 |

### `DiffHunk`

| フィールド | 型 | 説明 |
|------------|-----|------|
| `context_before` | Token[] | 変更箇所直前の意味のあるトークン |
| `removed` | Token[] | 削除されるトークン |
| `added_text` | string | 置換文字列（編集後ソースからそのまま抽出） |
| `context_after` | Token[] | 変更箇所直後の意味のあるトークン |

### `Token`

| フィールド | 型 | 説明 |
|------------|-----|------|
| `kind` | string | `CODE`, `STRING_LITERAL`, `LINE_COMMENT`, `BLOCK_COMMENT`, `NEWLINE`, `WHITESPACE` のいずれか |
| `text` | string | トークンのテキスト |

### 例

```json
{
  "version": "1",
  "id": "20260628-fix-null-check-a1b2c3d4",
  "author": "alice",
  "created_at": "2026-06-28T10:00:00+0900",
  "description": "fix null check",
  "target_file": "src/Foo.java",
  "language": "java",
  "encoding": "UTF-8",
  "hunks": [
    {
      "context_before": [],
      "removed": [],
      "added_text": "    Objects.requireNonNull(bar);\n",
      "context_after": []
    }
  ]
}
```

## 対応言語プロファイル

| プロファイル | 拡張子 |
|--------------|--------|
| Java | `.java` |
| Python | `.py` |
| C/C++ | `.c`, `.cpp`, `.cc`, `.cxx`, `.h`, `.hpp`, `.hxx`, `.rc` |
| SQL | `.sql` |
| JavaScript/TypeScript | `.js`, `.ts`, `.jsx`, `.tsx`, `.mjs`, `.cjs` |
| Rust | `.rs` |
| C# | `.cs`, `.csx` |
| Go | `.go` |
| PL/SQL | `.pls`, `.pks`, `.pkb`, `.pck`, `.psc`, `.plsql` |
| Generic（汎用） | 上記以外の拡張子 |

認識できない拡張子は汎用プロファイル（行コメント `//`、ブロックコメント `/* */`）が使われる。

## トラブルシューティング

| 問題 | 原因 / 対処 |
|------|-------------|
| **パッチリポジトリパスが設定されていない** | **設定** からパッチリポジトリパスを指定する |
| **work_dir が設定されていない** | **設定** から work ディレクトリを指定する |
| **開いているファイルが work_dir 配下にない** | 対象ファイルは設定した work ディレクトリ内にある必要がある |
| **変更が見つかりませんでした** | 原文と編集テキストが同一 |
| **ハンクが一意でない（生成時）** | 変更パターンが複数箇所にマッチ。前後のコードを含めて編集範囲を広げる |
| **ハンクが見つからない（適用時）** | 対象ソースが変化している。パッチを再生成するか手動で修正する |
| **曖昧なマッチ（適用時）** | 複数箇所にマッチ。手動確認が必要 |
| **対象ファイルが見つからない（バッチ）** | `work_dir` とパッチ内の `target_file` を確認する |
| **ファイルオープンエラー** | ファイルの存在と読み取り権限を確認する |
