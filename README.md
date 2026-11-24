# Hikidashi(引き出し)

**アプリ, ファイル, ブックマーク, 窓, プロセスを1つの入力欄から探して開く**Windows用のランチャー.

<!-- 画像: 打って開くまでの動くGIF(docs/images/demo.gif)をここに置く予定 -->

ホットキーで入力欄を出し, 名前の一部を打って`Enter`を押すと開きます. 飛び飛びの文字やローマ字でも探せて, パスを貼ればそのまま開き, 見つからなければChatGPTに聞けます. キーボードだけで, アプリもファイルもWebも開きたい人に向いています.

## できること

- **飛び飛びの文字で探す**: `pwpt`でPowerPoint, `kaigi`で「会議」の付くファイルが出ます. 前に開いたものは上に出ます.
- **まとめて探す**: アプリ, ユーザーフォルダーとWSLのファイル, ChromeとEdgeのブックマーク, Windowsの設定を1つの一覧に出します. Boxは`bx `のときだけ探します. Box Driveの手元のデータから読むので, 件数が多くてもすぐ探せます.
- **パスを貼って開く**: `C:\...`, `\\server\...`, `/mnt/c/...`, 引用符の付いたパスもそのまま読めます. `https://`のURLを貼ればブラウザーで開きます.
- **見つからなければChatGPT**: 一致がなければ, 打った文字をChatGPTに送ります. 一致があっても`Ctrl+Enter`でChatGPT. Google検索は`g `のあとに打つか`Ctrl+Alt+Enter`.
- **WSLとkill**: `w `でWSLのホームのファイルを探し, `k `でプロセスをすぐ終わらせます.
- **計算**: 16進数と2進数, dBmとW, VSWR, リアクタンスなどを打つだけで出します.
- **軽い**: exe 1つで動き, 管理者権限もインストールも要りません.

## はじめかた

1. [Releases](https://github.com/maonthemoon/hikidashi/releases/latest)から`Hikidashi-x.y.z.zip`をダウンロードし, 好きな場所に展開します.
2. `Hikidashi.exe`をダブルクリックします. 署名していないので, 「WindowsによってPCが保護されました」と出たら「詳細情報」→「実行」を押します. ZIPを展開する前に, ZIPのプロパティで「許可する」にチェックを入れると出にくくなります.
3. ホットキーを聞かれたら, 使いたいキー(例: `Alt+Space`)を押します. 次に, Windowsと一緒に起動するかを聞かれます.
4. ホットキーを押して, 名前の一部を打ち, `Enter`で開きます.

ZIPの中は, `Hikidashi.exe`, ローマ字で日本語の名前を探すための辞書`SKK-JISYO.L`とそのライセンス, `LICENSE.txt`, `THIRD-PARTY-NOTICES.txt`, `Manual.html`です. インストーラーも管理者権限も要りません.

- 「スマート アプリ コントロールが安全でない可能性のあるアプリをブロックしました」と出たら, Windowsセキュリティ →「アプリとブラウザーコントロール」→「スマートアプリコントロールの設定」で「オフ」にします. 署名のないexeは個別に許可できません.

### gitを使う場合

cloneして, `bin\Hikidashi.exe`を起動するだけで使えます(最新のリリースと同じexeです. ビルドは要りません).

```bat
cd %USERPROFILE%
git clone https://github.com/maonthemoon/hikidashi.git
```

管理者として開いたコマンドプロンプトは`C:\Windows\System32`から始まり, そこではcloneできません. 普通に開くか, 先に`cd %USERPROFILE%`(PowerShellなら`cd $HOME`)します.

### 更新する

新しいZIPの`Hikidashi.exe`をダブルクリックするだけです. 動いている古いものが終わり, 新しいものに入れ替わります. gitの場合は, `git pull`して`bin\Hikidashi.exe`をダブルクリックします.

Hikidashiは自分を`%LOCALAPPDATA%\Hikidashi\Hikidashi.exe`に写してから, そちらで動きます. なので, 動かしたまま新しい版を展開したり`git pull`したりできます. 設定と学習はそのまま残ります.

## 操作

| キー | 起きること |
|---|---|
| ホットキー | 窓を出す. もう一度押すと隠れる. 前回の入力が全選択で残る |
| `Enter` | 選んだ行を開く. 一致がなければChatGPTに聞く |
| `Shift+Enter` | 改行して長い質問を書く窓にする. 見出しや太字を整えて見せ, `Enter`でChatGPTに送る |
| `Ctrl+Enter` | 一致があっても, 打った文字をChatGPTに聞く |
| `Ctrl+Alt+Enter` | Googleで検索する |
| `Ctrl+Shift+Enter` | 管理者として実行する |
| `Alt+Enter` | ターミナル(WezTermかWindows Terminal)で, そのフォルダーに入る |
| `Tab` | 開き方を選ぶ(入っているフォルダーを開く, パスをコピー, VS Codeで開くなど) |
| `↑` `↓` | 行を選ぶ. 入力欄が空のときの`↑`は, 前に打った文字を呼び出す |
| ホイール, `PageUp` `PageDown` | 9行目からの結果を見る(窓は8行のままスクロールする) |
| クリック | その行を開く(`Tab`の一覧の行も) |
| `Esc` | 隠す(`Tab`の一覧なら元に戻る) |

### 種類を絞る

先頭に1文字と空白を打つと, その種類だけを探します. 入力欄が空のときに`Backspace`で戻ります.

| 先頭 | 探すもの |
|---|---|
| `a ` | アプリ |
| `f ` | ファイルとフォルダー |
| `b ` | ChromeとEdgeのブックマーク |
| `k ` | 終わらせるプロセス(`Enter`で確認なしに終わらせる) |
| `w ` | WSLのホームのファイルとフォルダー(WSLはこのときだけ読む) |
| `bx ` | Boxのファイルとフォルダー(Boxはこのときだけ探す) |
| `g ` | Googleで検索する(`Enter`で検索) |
| `c ` | ChatGPTに聞く長い質問の窓をすぐ開く |

### 計算

進数は, 打った進数以外の2つを出します: `0x1F`(16進数), `0b111011`(2進数), `1187`(10進数). RFは, `10dBm`(Wと50Ωでの電圧), `100mW`(dBm), `3dB`, `x2`, `vswr 1.5`, `rl 20`, `vswr 10 il 2dB`(損失を通して入力から見たVSWR), `10nH@2.4G`, `1pF@100M`, `10nH 1pF`(共振周波数). `Enter`で結果をコピーします.

### タスクトレイ

アイコンのクリックで窓を出します. 右クリックのメニューには, `Show Hikidashi`, `Change hotkey…`, `Start with Windows`, `Rebuild index`, `Settings`, `Manual`, `Quit`があります. 設定画面とこのメニューは, 設定の`Language / 言語`で日本語にできます(初めはWindowsの言語).

## 設定と保存されるもの

| 場所 | 中身 |
|---|---|
| `%APPDATA%\Hikidashi\config.toml` | 設定. トレイの`Settings`で設定画面が開き, `Save`ですぐ反映される. 接頭辞, アプリの別名, 自作コマンド, 探すフォルダーと外すフォルダー, ブラウザー(既定のブラウザーかChromeかEdge, 新しい窓かタブか)など. 入力欄に`settings`と打っても設定画面が開く. メモ帳で直すこともでき, 書き方はファイルの中のコメントにある |
| `%APPDATA%\Hikidashi\learn.tsv` | 学習(何を打って何を開いたか) |
| `%APPDATA%\Hikidashi\history.txt` | 入力の履歴 |
| `%APPDATA%\Hikidashi\state.txt` | ホットキー |
| `%APPDATA%\Hikidashi\hikidashi.log` | 索引の件数と時間, エラー |
| `%LOCALAPPDATA%\Hikidashi\Hikidashi.exe` | 動いているexe |

社内のURLやパスなど, 人に見せたくない値は`config.toml`にだけ書きます.

アンインストールは, トレイの`Start with Windows`を外して`Quit`してから, `%LOCALAPPDATA%\Hikidashi`, `%APPDATA%\Hikidashi`, 展開したフォルダー(gitで入れた人はcloneしたフォルダー)を消すだけです.

## マニュアル

詳しい使い方は, トレイの右クリックのメニューの`Manual`で開きます(exeに埋め込んだマニュアルを`%LOCALAPPDATA%\Hikidashi\manual.html`に書き出して, ブラウザーで開く). ZIPの`Manual.html`も同じものです.

## 動作環境

- Windows 10, Windows 11(管理者権限は不要)
- 追加で入れるものはありません. ChromeかEdge, VS Code, WezTerm, Windows Terminal, WSLは, あれば使います. ページは既定のブラウザーで開きます.

## 開発者向け

- **作り**: Rust + Win32. `crates/core`は検索, 並び順, 計算など, Windowsに依存しない部分(`cargo test`で確かめる). `crates/app`は画面とOSの部分.
- **ビルド**: Raspberry PiなどのLinuxで, `x86_64-pc-windows-gnu`向けにクロスコンパイルします(mingw-w64と, Rustのそのターゲットが要ります). `./tools/build-exe.sh`でテストを通し, exeを作ります. 開発中のexeは`out/`に出ます. `bin/Hikidashi.exe`はリリースした版なので上書きしません.
- **リリース**: `v`で始まるタグ(例: `v0.1.0`)をpushすると, GitHub ActionsがビルドとテストをしてZIPを作り, Releasesに置きます. あわせて, `bin/Hikidashi.exe`をそのexeに差し替えます. `bin/Hikidashi.exe`は手で更新しません.
- **マニュアル**: `docs/index.html`(1つのHTML). exeに埋め込み, トレイの`Manual`で開きます. 元のファイルは開発用の文書と一緒に置いていて, このリポジトリには入っていません.
- **アイコン**: `uv run tools/make-icon.py`で`assets/hikidashi.ico`を作り直します.
- **辞書**: ローマ字で日本語を探すための辞書(SKK-JISYO.L)はGPLなので, exeには入れず, ZIPに別のファイルとして同梱します(Actionsが[skk-dev/dict](https://github.com/skk-dev/dict)から取ってくる). exeの隣に辞書がなければ(gitで入れた場合など), 初めて起動したときにネットから取ってきます.

## ライセンス

[MIT License](LICENSE). exeに入るクレートなどの表記は`THIRD-PARTY-NOTICES.txt`にあり, 依存を変えたら`uv run tools/make-notices.py`で作り直します.
