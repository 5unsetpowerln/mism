## 対象範囲

mmmm は、Minecraft のサーバーを管理するためのコマンドラインツールである。
この仕様書は、mmmm の機能のうち、サーバーの新規作成、Minecraft のバージョンの列挙、ダウンロードした jar ファイルの再利用のための保存、サーバーの新規作成に伴う EULA への同意を定める。
mmmm が扱うサーバーは、PaperMC が配布している Paper と Folia である。
mmmm は、Linux の上で動く。

この仕様書の番号付きの文には、節の略号と連番からなる番号 (e.g. `new-3`)が付いている。
各テスト関数は、確かめる番号付きの文の番号を、そのテスト関数の `///` コメントに書く。

PaperMC は、Minecraft の各バージョン (e.g. `26.3`、`1.21.11`)に対して、Paper と Folia を配布している。
PaperMC は、1 つの Minecraft のバージョンに対する Paper (または Folia)に、番号で表したバージョン (e.g. `206`)を付け、そのバージョンごとに jar ファイルを配布している。
PaperMC は、Minecraft の各バージョンに、サポート状態 (SUPPORTED、DEPRECATED、UNSUPPORTED のいずれか)を付けて公開している。

## コマンドラインの形式

mmmm は、次の 2 つのサブコマンドを持つ。

```
mmmm new <paper|folia> <パス> [--version <Minecraft のバージョン>]
mmmm versions <paper|folia>
```

利用者は、Paper を `paper` という文字列で、Folia を `folia` という文字列で指定する。

- cli-1. 利用者が `mmmm --help` を実行したとき、mmmm は、mmmm の使い方を標準出力に出力し、終了ステータス 0 で終了する。
- cli-2. 利用者が `mmmm --version` を実行したとき、mmmm は、mmmm 自身のバージョンを標準出力に出力し、終了ステータス 0 で終了する。
- cli-3. 利用者が `mmmm new --help` または `mmmm versions --help` を実行したとき、mmmm は、そのサブコマンドの使い方を標準出力に出力し、終了ステータス 0 で終了する。
- cli-4. 利用者が上の形式に合わない引数を与えたとき、mmmm は、mmmm の使い方を標準エラー出力に出力し、終了ステータス 2 で終了する。
- cli-5. 利用者が <paper|folia> の位置に `paper` と `folia` 以外の文字列を与えたとき、mmmm は、cli-4 と同じ動作をする。

## サーバーの新規作成

### バージョンの選択

- sel-1. 利用者が `--version` を省略したとき、mmmm は、PaperMC がサポート状態 SUPPORTED を付けている Minecraft のバージョンのうち、最も新しくリリースされたものを選ぶ。
- sel-2. 利用者が `--version` に文字列を指定したとき、mmmm は、PaperMC が Paper (または Folia)を配布している Minecraft のバージョンのうち、その文字列と完全に一致するものを選ぶ。mmmm は、指定された文字列を別の表記に変換しない (e.g. `26.03` は `26.3` と一致しない)。
- sel-3. mmmm は、選んだ Minecraft のバージョンに対して PaperMC が配布している Paper (または Folia)のバージョンのうち、番号が最も大きいものを選ぶ。

### 作成の結果

Paper と Folia は、起動したディレクトリに `eula.txt` が無い状態で起動すると、そのディレクトリに `server.properties` と `eula=false` を書いた `eula.txt` を生成して終了し、ワールドを生成しない。
mmmm は、この動作を使って、利用者がサーバーを本格的に起動する前に、`server.properties` と `eula.txt` を用意する。

- new-1. 利用者が <パス> に相対パスを与えたとき、mmmm は、その相対パスを、カレントディレクトリを基準にした絶対パスとして扱う。
- new-2. `mmmm new` が成功したとき、<パス> はディレクトリである。
- new-3. `mmmm new` が成功したとき、そのディレクトリの直下に、mmmm が選んだ Paper (または Folia)のバージョンの jar ファイルが、PaperMC が配布している名前 (e.g. `paper-26.3-206.jar`)のまま 1 つある。
- new-4. mmmm は、jar ファイルを置いた後に、サーバーをそのディレクトリの中で 1 度だけ起動し、サーバーが終了するまで待つ。
- new-5. サーバーが起動している間、mmmm は、サーバーの標準出力と標準エラー出力の内容を、そのまま mmmm の標準エラー出力に出力する。
- new-6. `mmmm new` が成功したとき、そのディレクトリの直下の `eula.txt` は、`eula=true` という記述を含み、`eula=false` という記述を含まず、サーバーが書いた他の内容をそのまま含む。
- new-7. mmmm は、`eula.txt` を書き換える前に、利用者に確認を求めない。利用者は、`mmmm new` を実行することで Minecraft の EULA に同意したものとする。
- new-8. `mmmm new` が成功したとき、そのディレクトリの直下に `server.properties` がある。
- new-9. `mmmm new` が成功したとき、そのディレクトリの直下に `mmmm.toml` という名前の TOML 形式のファイルがあり、そのファイルは、mmmm がそのサーバーに Paper と Folia のどちらを使ったか、Minecraft のバージョン、Paper (または Folia)のバージョンの 3 つを記録している。
- new-10. `mmmm new` が成功したとき、mmmm は、`paper` か `folia`、Minecraft のバージョン、Paper (または Folia)のバージョン、作成したディレクトリの絶対パスをこの順に空白 1 文字で区切り、改行で終えた文字列 (e.g. `paper 26.3 206 /home/sunset/servers/survival`)を標準出力に出力し、終了ステータス 0 で終了する。

### エラー

- err-1. 利用者が指定した <パス> に、ファイル、ディレクトリ、シンボリックリンク (リンク先の無いものを含む)のいずれかが既に存在するとき、mmmm は、サーバーの作成を中止してエラーを報告する。
- err-2. 利用者が指定した <パス> の親ディレクトリが存在しないとき、mmmm は、サーバーの作成を中止してエラーを報告する。
- err-3. 利用者が `--version` に指定した文字列と一致する Minecraft のバージョンを PaperMC が配布していないとき、mmmm は、サーバーの作成を中止してエラーを報告する。
- err-4. 利用者が `--version` を省略し、サポート状態 SUPPORTED の Minecraft のバージョンが 1 つも無いとき、mmmm は、サーバーの作成を中止してエラーを報告する。
- err-5. mmmm が PaperMC との通信に失敗したとき、mmmm は、サーバーの作成を中止してエラーを報告する。
- err-6. mmmm がダウンロードした jar ファイルの SHA-256 が、PaperMC が公開している値と一致しないとき、mmmm は、サーバーの作成を中止してエラーを報告する。
- err-7. サーバーが終了した後に、`eula.txt` と `server.properties` の両方が作成中のディレクトリに揃っていないとき、mmmm は、サーバーの初回起動の失敗として、サーバーの作成を中止してエラーを報告する。
- err-8. mmmm がサーバーの作成を中止してエラーを報告したとき、利用者が指定した <パス> には何も存在しない。
- err-9. mmmm が実行を中止してエラーを報告するとき、mmmm は、何が失敗したか、その原因、利用者が取れる対処を標準エラー出力に出力し、終了ステータス 1 で終了する。この仕様書は、エラーの出力の長さを定めない。

### 中断

- int-1. `mmmm new` の実行中に mmmm が SIGINT (Ctrl-C)または SIGTERM を受けたとき、mmmm は、起動中のサーバーがあればそれを止め、利用者が指定した <パス> に何も残さずに終了する。

## バージョンの列挙

- ver-1. 利用者が `mmmm versions paper` (または `mmmm versions folia`)を実行したとき、mmmm は、PaperMC が Paper (または Folia)を配布しているすべての Minecraft のバージョンを、リリースが新しい順に、改行で区切って 1 つずつ標準出力に出力し、終了ステータス 0 で終了する。
- ver-2. ver-1 で mmmm が出力する各バージョンの記述は、Minecraft のバージョン、タブ 1 文字、PaperMC がそのバージョンに付けているサポート状態 (SUPPORTED、DEPRECATED、UNSUPPORTED のいずれか)をこの順につないだものである。
- ver-3. mmmm が PaperMC との通信に失敗したとき、mmmm は、標準出力に何も出力せず、バージョンの列挙を中止して err-9 と同じ動作をする。

## ダウンロードした jar ファイルの保存

mmmm は、ダウンロードした jar ファイルを、次に同じ jar ファイルが必要になったときに再利用するために、利用者のホームの下のディレクトリ (以下、この節ではキャッシュのディレクトリと呼ぶ)に保存する。

- cache-1. 環境変数 `XDG_CACHE_HOME` が設定されているとき、キャッシュのディレクトリは `$XDG_CACHE_HOME/mmmm` である。
- cache-2. 環境変数 `XDG_CACHE_HOME` が設定されていないとき、キャッシュのディレクトリは `~/.cache/mmmm` である。
- cache-3. mmmm が選んだ Paper (または Folia)のバージョンの jar ファイルがキャッシュのディレクトリに存在し、その SHA-256 が PaperMC が公開している値と一致するとき、mmmm は、その jar ファイルをダウンロードしない。
- cache-4. mmmm が選んだ Paper (または Folia)のバージョンの jar ファイルがキャッシュのディレクトリに存在し、その SHA-256 が PaperMC が公開している値と一致しないとき、mmmm は、その jar ファイルをダウンロードし直し、キャッシュのディレクトリにある jar ファイルを置き換える。
- cache-5. mmmm は、ダウンロードした jar ファイルの SHA-256 を PaperMC が公開している値と照合し、一致しないときは err-6 の動作をする。
- cache-6. mmmm は、キャッシュのディレクトリにある jar ファイルを、利用者の操作なしには削除しない。利用者が `mmmm new` を別の Minecraft のバージョンで実行した後も、前の Minecraft のバージョンの jar ファイルはキャッシュのディレクトリに残る。
