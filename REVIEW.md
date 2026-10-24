# urng v1.0.0 コードレビュー

- レビュー日: 2026年10月(改訂 2026-10-25)
- 対象: `d:\.repo\urng-review_kimi`(Rust RNG クレート `urng` v1.0.0、edition 2024 / MSRV 1.92)
- 方法: 実コード全量精読 + 広範な実行検証(リポジトリ外のスクラッチクレートで path 依存により検証。リポジトリ本体は一切変更なし)
- 検証環境: Windows(開発-box、AVX-512VL 対応 CPU)。VCS は全ファイル untracked(undcommitted)状態

---

## A. 主要な欠陥(正確性・安全性)

### A1.【Critical】`Xoshiro256Ssx2::nextu` — 64バイト書込みのバッファ超過(メモリ破壊)+ 安全 fn マーク

- `src/prng/b64/xoshiro.rs:169-171`:

  ```rust
  let mut out = [0u64; 2];
  _mm512_stream_si512(out.as_mut_ptr() as *mut __m512i, res);
  out
  ```

  - **16バイトのスタック配列 `[u64; 2]` に 64バイトの NT ストア**を書き込む(48バイト越境)。
  - 実測: **デバッグビルドで即時 `STATUS_ACCESS_VIOLATION (0xc0000005)`**。リリースビルドは最適化がストアを消し込むため偶然生残して意味不明な値を返す — つまり UB が optimizer 次第で顕在化するかどうかが決まる状態。
  - 同メソッドは `pub fn`(unsafe マークなし)にもかかわらず CPU 依存(及びアライメント要件を持つ)AVX-512 命令を含むため、シグネチャ側の安全性表記も不正(`unsafe` を付けるべき)。
  - 加えて、アルゴリズム自体が未達: 状態更新に `_mm512_shuffle_epi32` を使用しているが、VPSHUFD は **128ビット区間内の dword 並べ替え**しか行えず、スカラー `Xoshiro256Ss` の 4×u64 状態の更新(`s0^=s3` など区間をまたぐレーン混合)を構造的に表現できない。使用された imm はいずれも区間内 identity または u64 ペアのスワップ/反転のみ。
  - `unsafe_test!` 対象に含まれず、doctest も走らないため **全テスト緑でも全く検出されない**。README の AVX-512 テーブルに公に記載された公開型。

### A2.【High】「AVX2ティア」が実際には AVX-512VL 必須 — README/ドキュメントの ISA 分類が不正

README の `AVX2 (avx2)` テーブル(§ AVX2: `Sfc32x8` / `Jsf32x8` / `Xoroshiro64Ssx8`)に対して、実装は:

| 型 | 位置 | 問題の intrinsic | 実際に必要な ISA |
| --- | --- | --- | --- |
| `Jsf32x8::nextuv` | `src/prng/b32/jsf.rs:108-109` | `_mm256_rol_epi32` | **AVX-512F + AVX-512VL** (VPROLD ymm) |
| `Sfc32x8` | `src/prng/b32/sfc.rs:210` | `_mm256_rol_epi32` | 同上 |
| `Xoroshiro64Ssx8::nextuv` | `src/prng/b32/xoroshiro.rs:119,122` | `_mm256_rol_epi32` | 同上 |
| 共通 `WordV::to_randi`(__m256i) | `src/rngv.rs:77`, `src/prng/b32/sfc.rs:227` | `_mm256_mask_blend_epi32` | **AVX-512F + AVX-512VL** |

- これらの 256-bit ニーモニックは EVEX エンコードの `VPROLD`/`VPBLENDMD` で、**AVX2 専用 CPU(よくある Zen1/2、Coffee Lake 級、多くのクラウドインスタンス)では即 SIGILL(不正命令)**。
- 安全契約も不整合: 「CPUが avx2 をサポートし適切な target feature でコンパイル済み」という契約(`xoroshiro.rs:83` 等)を**満たしても** crash する。
- AVX2 だけで実行可能な置換表現が存在: rotate は `or(_mm256_slli_epi32, _mm256_srli_epi32)`、blend は `_mm256_blend_epi32`(imm8, AVX2)。
- ランタイム dispatch(`_internal.rs:508-526` `dispatch_simd!`)は `avx512f` のみ検出し `avx512vl` 未検査(512-bit 版の `rol` は AVX512F に含まれるため実害はないが、契約整合性のため VL 検査の追加を推奨)。

### A3.【High】`Shuffle::shuffle / shuffled` が無偏 Fisher–Yates ではない(naive shuffle バイアス)

- `src/shuffle.rs:51-54 / 70-73`:

  ```rust
  (0..len - 1).for_each(|i| {
      let j = self.nextu().to_index(len);   // 常に全範囲 [0, len)
      src.swap(i, j);
  });
  ```

  正しい昇順 FY は `j = i + rnd(len - i)`([i, len) から引く)。この実装は既に確定した部分を「巻き戻す」ため、順列分布が非一様になる古典的な naive shuffle バイアス。

- 実測(リリースビルド、Sfc32、40万回):
  - len=2: P(identity)=0.4996(一様なら 0.5000 — 偶然一致し単純テストを透過する)
  - **len=3: P(identity)=0.2227**(一様なら 0.1667、**+34% の過剰**)

- 修正: `(0..len - 1).for_each(|i| { let j = i + self.nextu().to_index(src.len() - i); ... })`。`shuffled` も同じバグ。

### A4.【High/Critical(package レベル)】`lib.rs` の `Readme.md` ケースミスマッチ

- `src/lib.rs:1`:

  ```rust
  #![cfg_attr(not(doctest), doc = include_str!("../Readme.md"))]
  ```

- 実ファイルは `README.md` のみ(glob/`git status` で確認済み。`Readme.md` は存在しない)。
- **Windows/WSL-drvfs のような大文字小文字非区別 FS では問題がマスクされ、case-sensitive な Linux/一部 macOS 環境では `cargo build` / `cargo doc` / `cargo test`(lib コンパイル段階)がすべてエラーで失敗**(`include_str!` の展開時)。
- 先に実施した `cargo check --target x86_64-unknown-linux-gnu` の通過は drvfs 上の実行の可能性が高い。**真の Linux checkout での再現確認を強く推奨**。docs.rs ビルドも同様に破れる危険。

### A5.【Medium】範囲 API の前提条件が未チェックで debug/release で挙動が割れる

実測(スクラッチクレート、デバッグビルド):

| 呼び出し | 観測値 |
| --- | --- |
| `randi(i32::MIN, i32::MAX)` | **997/2000 回が `attempt to add with overflow` パニック**。`src/rng.rs:59` の `+ min` が非 wrapping のため(range=2^32 のとき `(w*range)>>32 = w` となり `w as i32 + i32::MIN` が半分の状態で溢れる)。release では wrap により偶然正しく、debug のみ必ず落ちる |
| `randi(5, 1)`(max < min) | debug: `attempt to multiply with overflow` パニック / **release: 黙ってゴミ値**(range が u64 に負数変換され 2 の補数の巨大値になる) |
| `Word::to_index(len > u32::MAX)`(u32 word) | debug: 乗算溢れパニック / release: wrap → 範囲外 index → `choice()` で OOB の可能性 |

- 一様 `[min, max]` を約束する API として、前提違反時は **両プロファイルで一貫した明示的パニック**(または `wrapping_add` + ドキュメント)に統一すべき。現行は「debug のみ落ちる / release で無音に不正値」で最悪の形。
- 関連: SIMD の `to_randi`/`randiv`(`src/rngv.rs`、`sfc.rs:225-228` 等)は `_mm256_mul_epu32(v, scale)` の第二オペランドで **scale の偶数レーン**と x の奇数ワードがペアになる。C ABI 側は `_mm_set1_epi32` で broadcast しているため実用上正しいが、**per-lane の scale を渡すと暗黙に不正**。broadcast 前提の契約が一切文書化されていない。

### A6.【Medium】rand ブリッジ `SeedableRng::Seed = [u8; 4]` + `u32::from_ne_bytes`

- `src/_internal.rs:663-673`(`impl_rand_trait!`):
  - **64ビット状態のジェネレータ(Biski64・Xoshiro256++ 等)まで `[u8; 4]` シードに制限**(有効シード空間 2^32)。
  - `u32::from_ne_bytes` は **native endian 依存**で、BE プラットフォームとシード互換性がない。`from_le_bytes` への統一 + 64-bit 用 `[u8; 8]` 版の追加が必要。
- `try_next_u64` の 2語合成と `try_fill_bytes` の余り処理(`to_le_bytes` 先頭採取)は deterministic で体形的には妥当。

### A7.【Medium】その他の妥当性

- `choice` / `choice_mut`(`src/choice.rs:8-9`): 空スライスで **"index out of bounds: the len is 0 but the index is 0"** パニック(実測値)。`#Panics` 未文書。
- 同クレートの `shuffle` は空スライスを `ShuffleResult` の明示的エラーにしている — **同「空スライス」に panic / Err で API が不整合**。
- `Sampler`(Alias32/Bst32 等): 重み総和 0 や負の重みで**検証せず默って無号分布(NaN → prob 全 1.0 → 未加权等分)**を返す(実測 `[0,1,0,0]`)。入力検証なし。`alias: Vec<usize>` は u32 で十分(メモリ 2 倍の無駄)。
- `SeedGen`: RDSEED/RDRAND リトライ 4 回(仕様推奨は 10 回程度)。fallback は HighResクロック + seed ローテートの質の弱い出力。doc の "reproducibly" 表現は信用不可(混合結果は raw ハードウェア値依存で非再現)。
- PCG32: `inc` に `|1` を適用していない(`src/prng/b32/pcg.rs:25-33`)。周期性は保たれる(multiplier が oddなので全単射)が、公式 PCG のストリーム規約と出力系列が異なる。
- MT19937/SFMT 等も SplitMix by ユーザシードの独特な初期化で公式出力と非互換(次項 D の黄金ベクタ不在と絡む)。

---

## B. パフォーマンス

### 良い点

- `par_fill_reseed32/64`(`src/_internal.rs:352-420`): rayon で 512KB チャンク分割し、チャンクごとに `chunk_seed32`(golden ratio + SplitMix64 finalizer)で reseed する並列 bulk 生成。列の相関・alignment 問題を回避する体裁が良い。
- NT store (`fill_chunk_nt`, 24MB しきい値 / 32B アライメント fallback / `_mm_sfence` �除外、小チャンク fallback の regression test もある)は正確に設計されている。
- `fill_chunk` 4組アンロール、`impl_ring_rng32/64` による block buffering、`const fn new` + SplitMix seeding は idiomatic で速い構成。
- `bench.md`: bulk 生成はほぼ全型が 27 GB/s(書込み帯域 ceiling 99%) — 4-byte 生成がメモリ律速という正当な理解。L3 滞在モードで `jsf32x16` 50 GS/s、`cet64x8` 57.6 GS/s など SIMD 効果が明確。`threefish256` は 0.09 GS/s(明記済み)。

### 課題

- **`[[bench]]` が存在しない**: `criterion / plotters` が dev-dependency にあるが `benches/` がなく `cargo bench` は何もしない。bench.md は手作り markdown で再現手続がコードに紐づかない。
- ベンチ欠落: `xorshift32/64/128`、`xorwow`、`twisted_gfsr`、`wyhash64`、`SplitMix32`(scalar)、`xoroshiro128*`(scalar は一部掲載)など。shuffle/choice/sampler のベンチもなし。
- C ABI scalar bulk(`*_next_u32s` 等)は `par_fill_reseed32(buffer, rng.nextu(), …)` で**蓄積 state を使い捨てて都度 reseed**する設計。**bulk fill ストリームは scalar 連続呼出し列と一致しない**ことへの明示が README にない。
- A2 の AVX2 tier 不正のため、「AVX2-only 環境での SIMD 抗戦」が全体として機能していない。

---

## C. Rust 設計 / API

### 良い点

- `Word`(sealed, u32/u64)を起点に `to_f01 / to_randi / to_randf / to_index` の数値変換式を一箇所に集約し、`Choice / Shuffle` の blanket-impl(`<R: Rng + ?Sized>`)や SIMD(`RngV/WordV`)、portable(`wide::RngW`)がスカラー形と並行整合した設計。
- `impl_default_from_seed32/64` + `default_seed*`(時刻×カウンタ×SplitMix64 混ぜ)による `Default` 実装は idiomatic。
- `SplitMix64::compute` の `pub(crate)` 再利用(default シード・parallel チャンク reseed)は適切。
- `rand` モジュールは private のままにあり、`use urng::*` 併用时の名前衝突が避けられている。

### 課題

- **安全契約の文書不在/不正確**: A1(unsafe マークなしでメモリ破壊)、A2(契約を満たしても crash)。SIMD はユーザー API 側に runtime dispatch がなく(C ABI 側のみで検出)、呼び出し者責任の安全契約という設計自体は acceptable だが、tier の ISA 前提が文書と食い違っている。
- **API の不整合**: 空スライスの `choice`(panic) vs `shuffle`(Result) — A7。
- `Word::to_index` の doc「Equivalent to `to_randi(0, len as Int - 1)`」は len > u32::MAX で不正確(nit)。
- 死代码: `src/macros.rs` は 1バイトの空ファイルだが `#[macro_use] mod macros;` で読み込まている; `ambiguous_glob_reexports` allow は構造調整で消せる。
- `Cargo.toml`:
  - `exclude = ["bench.md", "build.rs", "test.nu"]` で **build.rs が公開パッケージから消える** → crates.io 利用者側で csbindgen による C# binding 再生成が不可能(ビルド自体は通る)。
  - `thiserror`(optional)が完全に未使用(feature にも紐付けられていない)。
  - `criterion / plotters` はベンチ不在の dev-dep。
  - `[lib] crate-type = ["cdylib", "rlib"]` は cabi 無効でも cdylib をビルドする。
- `build.rs` がビルドのたびに README.md を書き換える設計(DOCS_RS ガードあり)。一般公開にはやや不自然。

---

## D. テスト・ドキュメント

### テスト

- 実績(後述 E): Windows 上(default 48 unit + 44 doc / all-features 101 unit + 104 doc)すべて PASS。ただし:
  - **`tests/` integration test 全くなし**。ゴールデンベクタ(MT19937/SFMT/PCG/xoshiro 公式出力等)との交差認証が一切なく、PCG `inc|1` 省略や MT19937 独自 seeding の差異を検出できない。
  - `safe_test! / unsafe_test!` は「**同シードの同型インスタンスが同出力**」という実質同義反覆の決定論スモークのみ。分布品質(χ² 等)の外部 `cribler` 併用(README で主張)は未確認 · shuffle 偏りのような非一様性を捕捉しない。
  - **AVX2-only CPU(または RUSTFLAGS による AVX512VL OFF ビルド)の CI マトリクスがない** → A2 検出不能。
  - debug/release の振る舞い差(overflow パニック)も未検証。`Xoshiro256Ssx2` は doctest もテストもない。
  - 所々の SIMD 部分は `#[cfg(all(feature="simd", target_feature="avx512f"))]` で gated され、通常ビルドでは部分的に untested。

### ドキュメント

- README:
  - **AVX2 ティア分類の誤り**(A2)。
  - C ABI 例： `mt19937_new(uint32_t seed, size_t warm)` — 実 signature(`src/cabi/prng/b32/mersenne.rs:12`)は `warm` パラメータなし。
  - `Sfc64x8::nextu` の doc に「Generates 4 random `u64` values / at least 8 `u64`」とあるが、実仕様は `[u64; 8]` 返却(`src/prng/b64/sfc.rs:111-139`)で不整合。
  - wide テーブルの泄漏(`Threefry32x2x/32x4x`, `Squares32x4/8` 未記載)。
  - SIMD「requires AVX support at runtime」の記述は AVX-512VL という真の要件(C ABI 側でのみ runtime detect)を反映しておらず暧昧なまま。
  - README の `urng = "1.0.0"` は build.rs がビルド時書き換える設計だが、build.rs が package excluded のため**公開版では動かない**(手更新必須)。
- `SeedGen` doc の "reproducibly" 表現は非再現(raw ハードウェア値含む)、`Choice::choice` に #Panics セクションなし。
- README 32-bit 標準テーブルに `Sfc32x4` が混在(SIMD feature 必須の型)。
- build.rs がビルドのたびに README.md を改変する設計・repo は全ファイル untracked — publish 前の版管理/CI 整備が必須。

---

## E. 検証結果(実行コマンドと結果)

| 実行 | 結果 |
| --- | --- |
| `cargo test --locked`(default) | 48 unit + 44 doc 全 PASS(既往セッション) |
| `cargo test --locked --all-features` | 101 unit + 104 doc 全 PASS(同上) |
| `cargo check --locked --target x86_64-unknown-linux-gnu` | Windows / Linux(drvfs) 両方 PASS(同上; drvfs が case-insensitive な点に留意) |
| スクラッチクレート `probe_shuffle --release` | len=2 P(identity)=0.4996、**len=3 P(identity)=0.2227(一様 0.1667)→ バイアス確定** |
| `probe_randi`(debug) | **randi(i32::MIN, i32::MAX): 997/2000 回 overflow パニック** |
| `probe_misc`(debug) | `choice(空)=panic(index out of bounds)` / `randi(5,1)=panic(乗算 overflow)`(release はゴミ値) / `to_index(len>2^32)` 乗算 overflow / `Alias32([0,0])`=`[0,1,0,0]` 黙って等分 |
| `probe_ssx2`(debug) | **`Xoshiro256Ssx2::nextu` で `STATUS_ACCESS_VIOLATION (0xc0000005)`** |
| `probe_ssx2`(release) | クラッシュせず(最適化によりストアが消し込み済み)で不正な値を返す — UB 確定 |

- 検証方法: リポジトリ外の temp スクラッチクレート(urng に path 依存、features: rand/sampler/seedgen/simd/wide/cabi)を使用。リポジトリ内容は不変。検証後 temp 削除済み。
- **限界**: AVX2-only CPU での実走検証(ISA 判定は Intel 仕様書に基づく静的判定)、真の Linux ファイルシステムでの case-sensitive ビルド再現、Criterion による自家測定再実行は未実施。

---

## 総括

公開に値する骨格(trait 設計・並列/NT store bulk 生成・ジェネレータの実装範囲)を持つが、**ship-blocking な欠陥が 4 件**(A1〜A4)。推奨修正順：

1. `lib.rs:1` を `README.md`(正しい case)に修正。
2. `_mm256_rol_epi32` → `or(_mm256_slli_epi32, _mm256_srli_epi32)`、`_mm256_mask_blend_epi32` → `_mm256_blend_epi32`(imm8)へ置換(AVX2 解放)。
3. `Xoshiro256Ssx2::nextu` の参照/サイズを正す(またはスカラー生成へ返す)+ unsafe マーク。
4. `Shuffle` を `j = i + to_index(len - i)` に統一。
5. `randi / to_index` の前提確認 + `wrapping_add` 化、空 `choice` → `Option`、rand bridge `[u8;8]` / `from_le_bytes`。
6. AVX2-only・debug ビルド(+overflow) CI マトリクスとゴールデンベクタテストの導入、[[bench]] の再接続。
