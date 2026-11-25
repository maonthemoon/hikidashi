# Hikidashi(引き出し)

**アプリ, ファイル, ブックマーク, 窓, プロセスを1つの入力欄から探して開く**Windows用のランチャー.

<a href="https://maonthemoon.github.io/hikidashi/">
  <picture>
    <source media="(max-width: 767px) and (prefers-color-scheme: dark)" srcset="docs/images/demo-narrow-dark.webp">
    <source media="(max-width: 767px)" srcset="docs/images/demo-narrow-light.webp">
    <source media="(prefers-color-scheme: dark)" srcset="docs/images/demo-dark.webp">
    <img src="docs/images/demo-light.webp" alt="Hikidashiの動くデモ: アプリ, ローマ字で日本語, パスを貼る, ChatGPTに聞く, 長い質問, 計算">
  </picture>
</a>

[紹介のページ](https://maonthemoon.github.io/hikidashi/)では, 6つの場面をタブで選んで見られます.

ホットキーで入力欄を出し, 名前の一部を打って`Enter`を押すと開きます. 飛び飛びの文字やローマ字でも探せて, パスを貼ればそのまま開き, 見つからなければChatGPTに聞けます. キーボードだけで, アプリもファイルもWebも開きたい人に向いています.

## ほかのランチャーとの違い

Windowsのスタートメニューの検索や, PowerToys Runなどのランチャーと同じく「打って開く」道具です. そのうえで, 日本語の環境で毎日使うときに困ることを減らしています.

| こんなとき | Hikidashiでは |
|---|---|
| 日本語の名前のファイルを探したいが, いちいち日本語入力をオンにして変換するのが面倒 | `kaigi`と打てば「会議」の付く名前が出ます(ローマ字を辞書で漢字に広げて探す). 窓を出すと日本語入力は自動でオフになるので, 切り替えも要りません |
| メールやチャットで受け取ったパスを開きたい | 引用符付きのパス, `/mnt/c/...`, `\\wsl.localhost\...`, `%APPDATA%\...`を貼って`Enter`で開きます. エクスプローラーのアドレス欄に貼り直す必要はありません |
| 探しても見つからなかったので, ChatGPTに聞きたい | 一致がなければ, 打った文がそのままChatGPTに送られます. ブラウザーを開いて入力欄に貼り直す必要はありません |
| ChatGPTに長い質問を, 整理して書いてから送りたい | `Shift+Enter`で書く窓が開きます. `#`の見出し, `-`の箇条書き, `**太字**`, `` `コード` ``などのMarkdownは, 印を隠して整えた形で見せるので, 送る前に読み返しやすくなります. 書きかけは残り, `Ctrl+↑`で前に送った質問も呼び出せます |
| 16進数や2進数, dBmなどをさっと換算したい | `=`と空白で計算のモードに入り, `0x1F`で10進数と2進数, `10dBm`でWと50Ωでの電圧, `vswr 1.5`でリターンロスなどが出ます. リアクタンス(`10nH@2.4G`)や共振周波数(`10nH 1pF`)も計算でき, `Enter`で結果をコピーします. 電卓を開く必要はありません |
| Box DriveやWSLの中のファイルも同じ場所から探したい | `bx `でBox, `w `でWSLのホームを探します. Box Driveは手元のデータを読むので, 何万件あってもすぐ出ます. BoxのファイルのURLを貼ると, 手元のファイルとして開きます |
| Windowsをダークモードで使っている | ランチャーの窓, 書く窓, 設定画面も, Windowsのアプリのモードに合わせてダークになります. 設定でいつもライト, いつもダークにもできます |
| 常駐させるものは軽くしたい | exe 1つ(約6MB)で, インストールも管理者権限も要りません. 作者のPC(約1万6千ファイル)では, メモリ約20MB, 1回の検索は1ms以下でした |

## できること

- **飛び飛びの文字で探す**: `pwpt`でPowerPoint, `kaigi`で「会議」の付くファイルが出ます. 前に開いたものは上に出ます.
- **まとめて探す**: アプリ, ユーザーフォルダーとWSLのファイル, ChromeとEdgeのブックマーク, Windowsの設定を1つの一覧に出します. Boxは`bx `のときだけ探します. Box Driveの手元のデータから読むので, 件数が多くてもすぐ探せます.
- **パスを貼って開く**: `C:\...`, `\\server\...`, `/mnt/c/...`, 引用符の付いたパスもそのまま読めます. `https://`のURLを貼ればブラウザーで開きます.
- **見つからなければChatGPT**: 一致がなければ, 打った文字をChatGPTに送ります. 一致があっても`Ctrl+Enter`でChatGPT. Google検索は`g `のあとに打つか`Ctrl+Alt+Enter`.
- **クリップボードの履歴**: `cb `で, コピーした文字とファイルを探して貼ります(メモリの中だけ).
- **選択した文字で開く**: 文字を選んで`Ctrl+Alt+Space`で, その文字を入れて窓を出します.
- **ファイルを条件で絞る**: `*.pdf`, `見積*`, `ext:docx,xlsx`, `in:案件`, `modified:7d`.
- **スクリプトと拡張**: フォルダーに置いたPowerShellなどのスクリプトやJavaScriptの拡張を, 一覧から使えます. `Hikidashi.exe --query "文字"`でほかのアプリからも開けます.
- **WSLとkill**: `w `でWSLのホームのファイルを探し, `k `でプロセスをすぐ終わらせます.
- **計算**: 16進数と2進数, dBmとW, VSWR, リアクタンス, MIPI RFFEなど. `=`と空白で計算のモードに入ります.
- **軽い**: exe 1つで動き, 管理者権限もインストールも要りません.

## はじめかた

1. [Releases](https://github.com/maonthemoon/hikidashi/releases/latest)から`Hikidashi-x.y.z.zip`をダウンロードし, 好きな場所に展開します.
2. `Hikidashi.exe`をダブルクリックします. 署名していないので, 「WindowsによってPCが保護されました」と出たら「詳細情報」→「実行」を押します. ZIPを展開する前に, ZIPのプロパティで「許可する」にチェックを入れると出にくくなります.
3. ホットキーを聞かれたら, 使いたいキー(例: `Alt+Space`)を押します. 次に, Windowsと一緒に起動するかを聞かれます.
4. ホットキーを押して, 名前の一部を打ち, `Enter`で開きます.

ZIPの中は, `Hikidashi.exe`と3つのフォルダーです: `docs`(マニュアル), `data`(ローマ字で日本語の名前を探すための辞書), `licenses`(ライセンス). フォルダーごと置いてください. インストーラーも管理者権限も要りません.

- 「スマート アプリ コントロールが安全でない可能性のあるアプリをブロックしました」と出たら, Windowsセキュリティ →「アプリとブラウザーコントロール」→「スマートアプリコントロールの設定」で「オフ」にします. 署名のないexeは個別に許可できません.

### gitを使う場合

cloneして, `bin\Hikidashi.exe`を起動するだけで使えます(最新のリリースと同じexeで, 隣に辞書もあります. ビルドは要りません).

```bat
cd %USERPROFILE%
git clone https://github.com/maonthemoon/hikidashi.git
```

管理者として開いたコマンドプロンプトは`C:\Windows\System32`から始まり, そこではcloneできません. 普通に開くか, 先に`cd %USERPROFILE%`(PowerShellなら`cd $HOME`)します.

### 更新する

トレイのメニューの`Quit`で終了し, 新しいZIPを展開して`Hikidashi.exe`を起動します. gitの場合は, `Quit`→`git pull`→`bin\Hikidashi.exe`を起動です(動いている間はexeを上書きできないため).

置いた場所の`Hikidashi.exe`がそのまま動きます. 別のフォルダーの新しいexeを起動したときは, 動いている古いものが終わって入れ替わり, 自動起動の登録先も新しい場所に直ります. 設定と学習はそのまま残ります.

## 操作

| キー | 起きること |
|---|---|
| ホットキー | 窓を出す. もう一度押すと隠れる. 前回の入力が全選択で残る(左の印は外れる) |
| `Enter` | 選んだ行を開く. 一致がなければChatGPTに聞く |
| `Shift+Enter` | 改行して長い質問を書く窓にする. 見出しや太字を整えて見せ, `Enter`でChatGPTに送る |
| `Ctrl+Enter` | 一致があっても, 打った文字をChatGPTに聞く |
| `Ctrl+Alt+Enter` | Googleで検索する |
| `Ctrl+Shift+Enter` | 管理者として実行する |
| `Alt+Enter` | ターミナル(WezTermかWindows Terminal)で, そのフォルダーに入る |
| `Tab` | 選んだ行を入力欄に入れて, 続きを打つ(補完) |
| `→`(入力欄の終わりで) | 操作を選ぶ(ファイルをコピー, 入っているフォルダーを開く, パスをコピー, VS Codeで開くなど). `←`で戻る |
| `↑` `↓` | 行を選ぶ. 入力欄が空のときの`↑`は, 前に打った文字を呼び出す |
| ホイール, `PageUp` `PageDown` | 9行目からの結果を見る(窓は8行のままスクロールする) |
| クリック | その行を開く(`→`の一覧の行も) |
| `Esc` | 隠す(`→`の一覧なら元に戻る) |

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
| `cb ` | クリップボードの履歴 |
| `= ` | 計算 |

### 計算

`=`と空白を打つと計算のモードになります. 進数は, 打った進数以外の2つを出します: `0x1F`(16進数), `0b111011`(2進数), `1187`(10進数). RFは, `10dBm`(Wと50Ωでの電圧), `100mW`(dBm), `3dB`, `x2`, `vswr 1.5`, `rl 20`, `vswr 10 il 2dB`(損失を通して入力から見たVSWR), `10nH@2.4G`, `1pF@100M`, `10nH 1pF`(共振周波数). MIPI RFFEは, `mipi`のあとにビット列を打つとフレームとビットに分けて見せ, `mipi sa=6 addr=1C data=38`のように打つとビット列になります(順番は自由. `Tab`で補完). `Enter`で結果をコピーします.

### タスクトレイ

アイコンのクリックで窓を出します. 右クリックのメニューには, `Show Hikidashi`, `Change hotkey…`, `Start with Windows`, `Rebuild index`, `Settings`, `Manual`, `Quit`があります. 設定画面とこのメニューは, 設定の`Language`で日本語にできます(初めはWindowsの言語).

ランチャーの窓, 長い質問を書く窓, 設定画面, このメニューは, Windowsのアプリのモードがダークならダークになります. 設定の`Theme`で, いつもライト, いつもダークにもできます.

## 設定と保存されるもの

| 場所 | 中身 |
|---|---|
| `%APPDATA%\Hikidashi\config.toml` | 設定. トレイの`Settings`で設定画面が開き, 変えるとすぐ反映される. `Copy setup prompt`で, ChatGPTなどに設定を直してもらうための文をコピーできる. キーワード, エイリアス, 検索するフォルダーと除外するフォルダー, ブラウザー(既定のブラウザーかChromeかEdge, 新しい窓かタブか)など. 入力欄に`settings`と打っても設定画面が開く. メモ帳で直すこともでき, 書き方はファイルの中のコメントにある |
| `%APPDATA%\Hikidashi\learn.tsv` | 学習(何を打って何を開いたか) |
| `%APPDATA%\Hikidashi\history.txt` | 入力の履歴 |
| `%APPDATA%\Hikidashi\state.txt` | ホットキー |
| `%APPDATA%\Hikidashi\hikidashi.log` | 索引の件数と時間, エラー |
| `%LOCALAPPDATA%\Hikidashi` | 同梱の拡張の写しと, ネットから取ってきた辞書(exeの隣に辞書がないときだけ) |

社内のURLやパスなど, 人に見せたくない値は`config.toml`にだけ書きます.

アンインストールは, トレイの`Start with Windows`を外して`Quit`してから, `%LOCALAPPDATA%\Hikidashi`, `%APPDATA%\Hikidashi`, 展開したフォルダー(gitで入れた人はcloneしたフォルダー)を消すだけです.

## マニュアル

詳しい使い方は, トレイの右クリックのメニューの`Manual`で開きます. 必ず, exeと一緒に配られた`docs\manual.html`(ZIPではexeの隣, gitではリポジトリの`docs`)が開き, Webは見にいきません.

## 動作環境

- Windows 10, Windows 11(管理者権限は不要)
- 追加で入れるものはありません. ChromeかEdge, VS Code, WezTerm, Windows Terminal, WSLは, あれば使います. ページは既定のブラウザーで開きます.

## 開発者向け

- **作り**: Rust + Win32. `crates/core`は検索, 並び順, 計算など, Windowsに依存しない部分(`cargo test`で確かめる). `crates/app`は画面とOSの部分.
- **ビルド**: Raspberry PiなどのLinuxで, `x86_64-pc-windows-gnu`向けにクロスコンパイルします(mingw-w64と, Rustのそのターゲットが要ります). `./tools/build-exe.sh`でテストを通し, exeを作ります. 開発中のexeは`out/`に出ます. `bin/Hikidashi.exe`はリリースした版なので上書きしません.
- **リリース**: `v`で始まるタグ(例: `v0.1.0`)をpushすると, GitHub ActionsがビルドとテストをしてZIPを作り, Releasesに置きます. あわせて, `bin/Hikidashi.exe`をそのexeに差し替えます. `bin/Hikidashi.exe`は手で更新しません.
- **マニュアル**: `docs/manual.html`(1つのHTML). トレイの`Manual`で開きます. ZIPにも`docs\manual.html`として入れます. 書体も埋め込み, 開いてもWebを見にいきません(`tools/embed-fonts.py`). 元のファイルは開発用の文書と一緒に置いていて, このリポジトリには入っていません.
- **アイコン**: `uv run tools/make-icon.py`で`assets/hikidashi.ico`を作り直します.
- **紹介のページ**: GitHub Pagesのトップ(`docs/index.html`)は, 窓をHTMLで作り直した動くデモのページです. `./tools/build-manual.sh`でマニュアル(`docs/manual.html`)と一緒に作ります. デモの元は紹介のページとマニュアルで同じです. READMEの動く画像は, `uv run tools/make-demo.py`でそのデモを`docs/images/demo-*.webp`に書き出します(ライトとダーク, 広い幅と狭い幅の4枚. Playwrightのchromiumを使う).
- **辞書**: ローマ字で日本語を探すための辞書(SKK-JISYO.L)はGPLなので, exeには入れず, 別のファイルとして`bin/`に置き, ZIPにも入れます. 新しくするときは`./tools/update-dict.sh`で[skk-dev/dict](https://github.com/skk-dev/dict)から取ってきてcommitします. exeの隣(ZIPでは`data`フォルダー)に辞書がなければ, 起動したときにネットから取ってきます.

## ライセンス

[MIT License](LICENSE). exeに入るクレートなどの表記は`THIRD-PARTY-NOTICES.txt`にあり, 依存を変えたら`uv run tools/make-notices.py`で作り直します.
