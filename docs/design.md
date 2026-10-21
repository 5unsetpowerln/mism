## 対象範囲

mism は、Rust で実装した 1 つの実行ファイルであり、仕様書 (`docs/spec.md`)が定める振る舞いを Linux の上で実現する。
この設計書は、仕様書のサーバーの新規作成、バージョンの列挙、出力の規則、jar ファイルの保存、プラグインの管理、データパックの管理の番号付きの文 (`cli-1` から `drm-3` まで)をすべて満たす方法を定める。
この設計書の書き方は、`docs/development-rules.md` の「文書の規則」に従う。
この設計書では、`26.3` のような文字列を Minecraft のバージョンと呼び、PaperMC が 1 つの Minecraft のバージョンに対して番号 (e.g. `206`)を付けて配布する Paper (または Folia)を、その番号で識別して Paper (または Folia)のバージョンと呼ぶ。
この設計書で「筆者」と書くときは、この設計書を書いた者を指す。

## 全体の構成

### ファイルの分け方

```
Cargo.toml
src/
  main.rs                   サブコマンドの振り分け、エラーの出力、終了ステータスの決定
  command_line.rs           clap によるコマンドライン引数の定義 (mism plugin と mism datapack のサブコマンドの引数と使い方の説明は別々に定義する)
  output.rs                 mism 自身の出力 (各行への `mism: ` の付与、外部のソフトウェアの出力の始まりと終わりを伝える文字列、端末のときの色、plg-9 と dp-12 の形の記述、問い)
  prompt.rs                 標準入力からの回答の読み取り (標準入力が端末かの判定と、回答を待つ間の中断のフラグの確認)
  subcommands/
    mod.rs
    new.rs                  mism new の手順の全体
    versions.rs             mism versions の手順の全体
    target_server.rs        mism plugin と mism datapack のサブコマンドが使う、対象のサーバーの決定
    terminal_asker.rs       mism plugin と mism datapack のサブコマンドが Modrinth のファイルの選択に渡す、標準入力で値を尋ねる処理
    modrinth_search.rs      mism plugin search と mism datapack search が使う、件数の確かめと、Modrinth の検索と、見つかったプロジェクトのバージョンの一覧の取得
    modrinth_versions.rs    mism plugin versions と mism datapack versions が使う、Modrinth のバージョンの一覧の取得と、必須の依存先の識別子の解決と、新しい順の並べ替え
    modrinth_fetch.rs       mism plugin と mism datapack の取得の手順が使う、Modrinth のファイルの選択と、キャッシュのディレクトリに記録する値の決定
    plugin/
      mod.rs                mism plugin のサブコマンドの振り分けと、それらが共有する値 (ファイルの名前の末尾の .jar、キャッシュのディレクトリの中の plugins/、Modrinth の検索とファイルの選択の条件)と、変更が反映される旨の出力
      search.rs             mism plugin search の手順の全体
      versions.rs           mism plugin versions の手順の全体
      fetch.rs              mism plugin fetch の手順の全体と、inspect と install が使う取得の手順
      inspect.rs            mism plugin inspect の手順の全体と、place と install が使う検証の手順
      place.rs              mism plugin place の手順の全体と、install が使う配置の手順
      install.rs            mism plugin install の手順の全体
      list.rs               mism plugin list の手順の全体
      remove.rs             mism plugin remove の手順の全体
    datapack/
      mod.rs                mism datapack のサブコマンドの振り分けと、それらが共有する値 (ファイルの名前の末尾の .zip、キャッシュのディレクトリの中の datapacks/、Modrinth の検索とファイルの選択の条件)と、変更が反映される旨の出力
      search.rs             mism datapack search の手順の全体
      versions.rs           mism datapack versions の手順の全体
      fetch.rs              mism datapack fetch の手順の全体と、inspect と install が使う取得の手順
      inspect.rs            mism datapack inspect の手順の全体と、place と install が使う検証の手順
      place.rs              mism datapack place の手順の全体と、install が使う配置の手順
      install.rs            mism datapack install の手順の全体
      list.rs               mism datapack list の手順の全体
      remove.rs             mism datapack remove の手順の全体
  http_client.rs            HTTP の送受信 (JSON の取得と、jar ファイルとデータパックのファイルのストリームでの受信)。User-Agent とタイムアウトもここで決める
  interruption.rs           SIGINT と SIGTERM のハンドラの登録と、受信したシグナルを記録するフラグと、ファイルを変え始める直前のフラグの確認
  atomic_fs.rs              書き込みが終わるまで名前を持たないファイルの作成、既存の名前を上書きしない名前の変更、ファイルの置き換え、作業用の名前の生成
  minecraft_version.rs      Minecraft のバージョンの解析と、新旧の比較と、Minecraft のバージョンの一覧の中で最も新しいものの決定
  papermc.rs                Paper のプロジェクトの名前と、Paper にだけ必要な処理。API の呼び出しは papermc_api を使う
  folia.rs                  Folia のプロジェクトの名前と、Folia にだけ必要な処理。API の呼び出しは papermc_api を使う
  papermc_api/
    mod.rs                  PaperMC の API を呼ぶ処理のトレイトと、HTTP で呼ぶ実装。Paper と Folia で共通
    response.rs             API のレスポンスの JSON に対応する型
    selection.rs            Minecraft のバージョンと Paper (または Folia)のバージョンの選択
  modrinth_api/
    mod.rs                  Modrinth の API を呼ぶ処理のトレイトと、HTTP で呼ぶ実装。必須の依存先の識別子の解決
    response.rs             API のレスポンスの JSON に対応する型
    selection.rs            取得元の指定が指すファイルの絞り込みと、plg-12 (または dp-6)の形の取得元の指定の組み立て。候補にするファイルの条件と絞る属性の一覧は、呼び出す側から受け取る
  modrinth_source.rs        `modrinth:` の取得元の指定の解析と組み立て、識別子の文字の検査、取得元の指定からの識別子とバージョンの番号の取り出し
  fetch_source.rs           <取得元の指定> の 3 つの形への振り分け、URL からの名前の作成、手元のファイルの名前の検査。ファイルの名前の末尾は、呼び出す側から受け取る
  file_name.rs              外部から得たプラグインとデータパックの名前と、ファイルの名前の、パスの要素としての検査
  fetched_file_cache.rs     キャッシュのディレクトリの中の 1 つのディレクトリ (その名前は呼び出す側から受け取る)の下への、取得したファイルの保存と、そこからの探索と一覧
  plugin_jar.rs             jar ファイルの中の plugin.yml と paper-plugin.yml の読み取り
  plugin_compatibility.rs   プラグインの検証の値と出どころ、対象のサーバーとの照らし合わせ、プラグインの検証の判定
  datapack_zip.rs           データパックのファイル (zip 形式)の最上位の pack.mcmeta の読み取り
  datapack_compatibility.rs データパックの検証の値、対象のサーバーとの照らし合わせ、データパックの形式の番号の比較、データパックの検証の判定
  cache_directory.rs        キャッシュのディレクトリの決定
  content_hash.rs           SHA-256 と SHA-512 の計算と、16 進数の文字列への変換
  jar_cache.rs              Paper (または Folia)の jar ファイルのキャッシュのディレクトリへの保存と取り出し
  server_directory/
    mod.rs                  サーバーのディレクトリを表す型 (パスと mism.toml の内容)と、カレントディレクトリの mism.toml の読み込み
    creation.rs             作業用のディレクトリの作成と、指定されたパスへの配置
    initial_launch.rs       初回起動 (java の起動、終了の待機、eula.txt の書き換え)
    mism_toml.rs            mism.toml の型と読み書き (プラグインの記録とデータパックの記録を含む)
    placed_files.rs         置くディレクトリ (その場所は呼び出す側から受け取る)へのファイルの配置と削除と、mism.toml の記録との突き合わせ
    plugins.rs              プラグインを置くディレクトリ (plugins/)の決定と、plugins/ の jar ファイルの列挙と宣言の読み取り
    datapacks.rs            データパックを置くディレクトリ (level-name の値が決めるワールドのディレクトリの datapacks/)の決定
    server_properties.rs    server.properties の読み取りと、level-name の値の取り出し
    server_jar.rs           versions/ の下のサーバーの jar ファイルの version.json からの、データパックの形式の番号の読み取り
tests/                      結合テスト。ファイルの分け方は「テスト」の節で定める
```

ディレクトリを持つモジュールは、`mod.rs` をその入口にする。

### モジュール同士の依存の向き

- `main.rs` は、`command_line.rs`、`interruption.rs`、`output.rs`、`subcommands/` に依存する。`main.rs` に依存するモジュールは無い。
- `subcommands/` の各モジュール、`server_directory/initial_launch.rs`、`prompt.rs` は、`output.rs` に依存する。`output.rs` は、他のモジュールに依存しない。
- `subcommands/mod.rs` は、`command_line.rs`、`papermc.rs`、`folia.rs` に依存する。`subcommands/new.rs` は、`http_client.rs`、`papermc_api/`、`jar_cache.rs`、`server_directory/` に依存する。`subcommands/versions.rs` は、`http_client.rs`、`papermc_api/`、`minecraft_version.rs` に依存する。`subcommands/` に依存するモジュールは `main.rs` だけである。
- `subcommands/plugin/` は、`command_line.rs`、`subcommands/target_server.rs`、`subcommands/terminal_asker.rs`、`subcommands/modrinth_search.rs`、`subcommands/modrinth_versions.rs`、`subcommands/modrinth_fetch.rs`、`prompt.rs`、`http_client.rs`、`modrinth_api/`、`modrinth_source.rs`、`fetch_source.rs`、`file_name.rs`、`fetched_file_cache.rs`、`minecraft_version.rs`、`plugin_jar.rs`、`plugin_compatibility.rs`、`server_directory/mod.rs`、`server_directory/mism_toml.rs`、`server_directory/placed_files.rs`、`server_directory/plugins.rs` に依存する。
- `subcommands/datapack/` は、`command_line.rs`、`subcommands/target_server.rs`、`subcommands/terminal_asker.rs`、`subcommands/modrinth_search.rs`、`subcommands/modrinth_versions.rs`、`subcommands/modrinth_fetch.rs`、`prompt.rs`、`http_client.rs`、`modrinth_api/`、`modrinth_source.rs`、`fetch_source.rs`、`file_name.rs`、`fetched_file_cache.rs`、`minecraft_version.rs`、`datapack_zip.rs`、`datapack_compatibility.rs`、`server_directory/mod.rs`、`server_directory/mism_toml.rs`、`server_directory/placed_files.rs`、`server_directory/datapacks.rs`、`server_directory/server_jar.rs` に依存する。
- `subcommands/target_server.rs` は、`server_directory/mod.rs` に依存する。`subcommands/terminal_asker.rs` は、`prompt.rs` と `modrinth_api/selection.rs` に依存する。`subcommands/modrinth_search.rs` と `subcommands/modrinth_versions.rs` は、`http_client.rs` と `modrinth_api/` に依存する。`subcommands/modrinth_fetch.rs` は、`http_client.rs`、`modrinth_api/`、`modrinth_source.rs`、`fetched_file_cache.rs` に依存する。
- `papermc.rs` と `folia.rs` は、他のモジュールに依存しない。`papermc_api/` は、`http_client.rs` と `minecraft_version.rs` に依存する。
- `modrinth_api/mod.rs` は、`http_client.rs` に依存する。`modrinth_api/selection.rs` は、`modrinth_api/response.rs` と `modrinth_source.rs` に依存し、`prompt.rs` には依存しない。`modrinth_api/selection.rs` は、利用者に値を尋ねる処理を、`subcommands/terminal_asker.rs` からトレイトとして受け取る。
- `fetch_source.rs` は、`modrinth_source.rs` と `file_name.rs` に依存する。`plugin_compatibility.rs` は、`minecraft_version.rs`、`plugin_jar.rs`、`fetched_file_cache.rs` に依存する。`datapack_compatibility.rs` は、`datapack_zip.rs` と `fetched_file_cache.rs` に依存する。
- `jar_cache.rs` は、`cache_directory.rs`、`content_hash.rs`、`http_client.rs`、`atomic_fs.rs` に依存する。`fetched_file_cache.rs` は、`cache_directory.rs`、`content_hash.rs`、`http_client.rs`、`atomic_fs.rs`、`file_name.rs`、`modrinth_source.rs` に依存する。
- `server_directory/creation.rs` は、`atomic_fs.rs` と `server_directory/initial_launch.rs` と `server_directory/mism_toml.rs` に依存する。`server_directory/mism_toml.rs` は、`atomic_fs.rs` に依存する。`server_directory/placed_files.rs` は、`atomic_fs.rs`、`content_hash.rs`、`file_name.rs`、`server_directory/mod.rs`、`server_directory/mism_toml.rs` に依存する。`server_directory/plugins.rs` は、`plugin_jar.rs`、`server_directory/mod.rs`、`server_directory/mism_toml.rs`、`server_directory/placed_files.rs` に依存する。`server_directory/datapacks.rs` は、`server_directory/server_properties.rs`、`server_directory/mod.rs`、`server_directory/mism_toml.rs`、`server_directory/placed_files.rs` に依存する。
- `http_client.rs`、`prompt.rs`、`modrinth_api/mod.rs`、`fetched_file_cache.rs`、`subcommands/plugin/`、`subcommands/datapack/`、`server_directory/initial_launch.rs`、`server_directory/creation.rs` は、中断のフラグを確認するので `interruption.rs` に依存する。エラーに受信したシグナルの種類 (`interruption.rs` の `Signal`)を持たせるモジュール (`jar_cache.rs`、`fetched_file_cache.rs`、`papermc_api/`、`modrinth_api/`、`subcommands/`)も、`interruption.rs` に依存する。
- `interruption.rs`、`atomic_fs.rs`、`minecraft_version.rs`、`modrinth_source.rs`、`file_name.rs`、`plugin_jar.rs`、`datapack_zip.rs`、`cache_directory.rs`、`content_hash.rs`、`server_directory/server_properties.rs`、`server_directory/server_jar.rs` は、他のモジュールに依存しない。
- プラグインだけが使うモジュール (`subcommands/plugin/`、`plugin_jar.rs`、`plugin_compatibility.rs`、`server_directory/plugins.rs`)と、データパックだけが使うモジュール (`subcommands/datapack/`、`datapack_zip.rs`、`datapack_compatibility.rs`、`server_directory/datapacks.rs`、`server_directory/server_properties.rs`、`server_directory/server_jar.rs`)は、互いに依存しない。

### 複数のモジュールに制約を課す決定

- mism は、非同期ランタイムを持たず、mism 自身のコードではスレッドを作らず、すべての処理を 1 つのスレッドで順に行う。mism が行う通信は、1 つずつ順に送る GET のリクエスト (JSON の取得と、jar ファイルとデータパックのファイルのダウンロード)だけであり、mism が同時に待つ入出力は無いからである。最も多くのリクエストを送る `mism plugin search` と `mism datapack search` も、リクエストを 1 つずつ順に送る (「mism plugin search と mism datapack search の手順」の節)。mism が ureq に通信の合計の時間の上限を設定すると、ureq は、名前解決のためのスレッドを内部で作る。筆者は、このことを 2026 年 10 月 20 日に確認した。
- 各モジュールは、そのモジュールのエラーの列挙型を thiserror で定義し、そのモジュールのファイルの中に置く。各バリアントは、メッセージに必要な値 (パス、URL、Minecraft のバージョンなど)と、元になったエラーを `#[source]` で持つ。`main.rs` は、各モジュールのエラーを `#[from]` で包む最上位の列挙型を持つ。
- mism は、テストのために、mism が通常使う値を上書きする環境変数を 3 つ持つ。`MISM_PAPERMC_URL` は PaperMC の API のベース URL を、`MISM_MODRINTH_URL` は Modrinth の API のベース URL を、`MISM_DOWNLOAD_IDLE_TIMEOUT` はダウンロードを打ち切るまでの、データが届かない状態が続いてよい秒数を上書きする。この 3 つの環境変数が設定されていなければ、mism は通常の値を使う。
- mism 自身の出力 (標準出力への行の書き込み)は、すべて `output.rs` を通して行う。`output.rs` は、書き込む文字列の各行の先頭に `mism: ` を付ける (out-1)。mism 自身は、標準エラー出力に何も書かない (out-4)。clap が出力する使い方とバージョン (cli-1 から cli-5)は、clap が直接書くので、`mism: ` を付ける処理の対象外であり、cli-4 と cli-5 の使い方だけが標準エラー出力に出る。mism が利用者に回答を求めるときに書く問いの文字列も、`output.rs` を通して書く。
- `mism plugin` と `mism datapack` のサブコマンドが値を並べて書く記述は、すべて `output.rs` が plg-9 と dp-12 の形 (2 つは同じ形である)に組み立てる (plg-9、dp-12)。一覧の値の要素を、mism は、重複を除いたうえで、Minecraft のバージョンの一覧では Modrinth が返した順に、それ以外の一覧では文字列の昇順 (UTF-8 のバイト列としての比較)に並べる。
- mism は、標準入力からの読み取りをすべて `prompt.rs` を通して行う。`prompt.rs` は、標準入力から回答を改行ごとに 1 つずつ読み、標準入力が端末かどうかを `std::io::IsTerminal` で判定する。
- `interruption.rs` は、`SIGINT` と `SIGTERM` のハンドラと、受信したシグナルの種類を記録するプロセス全体で共有するフラグを持つ。`main.rs` は、起動時にそのハンドラを登録する。ハンドラは、フラグにシグナルの種類を記録するだけである。時間のかかる処理 (ダウンロード、Minecraft のサーバーの終了の待機、利用者の回答の待機、Modrinth へのリクエストの繰り返し)を行うモジュールは、このフラグを確認し、フラグが立っていれば処理をやめて中断のエラーを返す。
- `mism plugin` と `mism datapack` のサブコマンドは、キャッシュのディレクトリ、対象のサーバーのプラグイン (またはデータパック)を置くディレクトリ (`plugins/`、またはワールドのディレクトリとその `datapacks/`)、`mism.toml` のどれかを変え始める直前に中断のフラグを確認し、フラグが立っていれば何も変えずに中断のエラーを返す。サブコマンドは、それらを変え始めた後はフラグを確認せず、変える処理を最後まで行う。
- mism は、Modrinth の API、URL、利用者の回答、手元のファイルのパス、`mism.toml`、`source.toml` から得たプラグインとデータパックの名前と、jar ファイルとデータパックのファイルの名前を、ファイルやディレクトリの名前に使う前に `file_name.rs` で検査する (「ファイルの名前の検査」の節)。
- mism は、既にあるファイル (`mism.toml` とキャッシュのディレクトリの `source.toml`)を書き換えるとき、`atomic_fs.rs` の置き換えを使う。
- mism は、プラグインとデータパックを、別々のモジュールで扱う。プラグインとデータパックの両方に共通の処理は、プラグインだけが使うモジュールにもデータパックだけが使うモジュールにも依存しないモジュール (`modrinth_api/`、`modrinth_source.rs`、`fetch_source.rs`、`file_name.rs`、`fetched_file_cache.rs`、`server_directory/placed_files.rs`、`subcommands/target_server.rs`、`subcommands/terminal_asker.rs`、`subcommands/modrinth_search.rs`、`subcommands/modrinth_versions.rs`、`subcommands/modrinth_fetch.rs`)にあり、プラグインとデータパックで違う値 (ファイルの名前の末尾、置くディレクトリ、キャッシュのディレクトリの中のディレクトリの名前、`mism.toml` の記録の配列、Modrinth の検索とファイルの選択の条件)を、呼び出す側のモジュールから受け取る (「mism plugin と mism datapack の対象のサーバーと共通の手順」の節)。両方が使うモジュールのエラーは、`mism plugin` と `mism datapack` のどちらのサブコマンドも案内しない。サブコマンドを案内する文 (e.g. `mism plugin search` で識別子を探す案内)は、`subcommands/plugin/` と `subcommands/datapack/` が、それぞれのエラーの案内として持つ。mism がプラグインとデータパックを別々のモジュールで扱うのは、利用者にとって 2 つが別の機能であり、1 組のモジュールで 2 つを扱うと、片方の都合で、もう片方の使い方の説明やエラーの文が曖昧になるからである。

### 追加の機能の置き場所

- 変更を行う者は、新しいサブコマンド (起動、停止、更新、バックアップ)を実装するファイルを、`subcommands/` に 1 つずつ追加する。
- 変更を行う者は、新しいサーバーソフトウェア (e.g. Spigot)を扱うファイル (`spigot.rs`)を `papermc.rs` と同じ階層に追加し、配布元の API を呼ぶモジュールを `spigotmc_api/` のように配布元の名前のディレクトリに置く。複数のサーバーソフトウェアに共通して必要になった型は、変更を行う者が、2 つ目のサーバーソフトウェアを追加する時点で、それらのファイルから共通のモジュールに切り出す。
- 変更を行う者は、`mism plugin` の新しいサブコマンド (e.g. 更新)を実装するファイルを `subcommands/plugin/` に、`mism datapack` の新しいサブコマンドを実装するファイルを `subcommands/datapack/` に、1 つずつ追加する。変更を行う者は、mism が扱うファイルの新しい種類 (e.g. リソースパック)を、`subcommands/resourcepack/` のように種類の名前のディレクトリに置き、両方が使うモジュールを使って実装する。
- 変更を行う者は、プラグインとデータパックの取得元 (e.g. Hangar)を追加するとき、その取得元の API を呼ぶモジュールを `hangar_api/` のように取得元の名前のディレクトリに置き、取得元の指定の新しい形を `fetch_source.rs` に足し、HTTP の送受信に `http_client.rs` を使い、ファイルの保存に `fetched_file_cache.rs` を使う。
- 変更を行う者は、サーバーのプロセスの起動と停止と、`server.properties` の編集を実装するファイルを、`server_directory/` に追加する。

## 依存するクレート

- mism は、clap を、コマンドライン引数の定義と解析に使う。筆者が clap を選んだのは、clap が、形式に合わない引数で使い方を標準エラー出力に出して終了ステータス 2 で終わる動作 (cli-4)を持ち、mism がそれをそのまま使えるからである。
- mism は、ureq を、HTTP の送受信に使う。筆者が ureq を選んだのは、ureq が同期のクライアントなので非同期ランタイムを持たないという決定に合い、ureq が TLS に rustls を使うので OpenSSL のライブラリが無い環境でも mism が動き、ureq がレスポンスの本文をストリームとして読めるので mism がダウンロードしながら SHA-256 を計算できるからである。
- mism は、serde と serde_json を、PaperMC と Modrinth の API のレスポンスの JSON と、データパックの `pack.mcmeta` とサーバーの jar ファイルの `version.json` (どちらも JSON 形式)を型に読み込むために使う。筆者がこの 2 つを選んだのは、serde と serde_json が、Rust で JSON を構造体に対応させる方法として定着しているからである。
- mism は、toml を、`mism.toml` とキャッシュのディレクトリの `source.toml` の読み書きに使う。筆者が toml を選んだのは、toml が、serde と組み合わせて構造体と TOML を対応させられるからである。
- mism は、sha2 を、jar ファイルの SHA-256 の計算と、Modrinth が公開する SHA-512 と照合するための SHA-512 の計算に使う。筆者が sha2 を選んだのは、RustCrypto が sha2 を保守しており、mism が jar ファイルをストリームで読みながら SHA-256 と SHA-512 を計算できるからである。sha2 は SHA-512 も提供するので、mism は、SHA-512 のために別のクレートを足さない。
- mism は、thiserror を、各モジュールのエラーの列挙型の定義に使う。筆者が thiserror を選んだのは、thiserror を使うと、mism が `#[source]` と `#[from]` で原因の連鎖を型に持たせられるからである。
- mism は、nix を、名前を持たないファイルの作成 (`O_TMPFILE`)、そのファイルへの名前の付与 (`linkat`)、既存の名前を上書きしない名前の変更 (`renameat2`)、Minecraft のサーバーのプロセスへのシグナルの送信、標準入力が読めるようになるまでの待機 (`poll`)と標準入力からの読み取り (`read`)に使う。筆者が nix を選んだのは、nix が生の libc の関数に型の付いた安全なラッパーを提供し、mism が `unsafe` を書かずに済むからである。変更を行う者は、`poll` のために nix の `poll` の機能 (Cargo の feature)を有効にする。Minecraft のサーバーのプロセスのプロセスグループの設定には、mism は、標準ライブラリの `CommandExt::process_group` を使う。
- mism は、zip を、プラグインの jar ファイル (zip 形式)の中の `plugin.yml` と `paper-plugin.yml` の読み取りと、データパックのファイル (zip 形式)の最上位の `pack.mcmeta` の読み取りと、サーバーの jar ファイル (zip 形式)の中の `version.json` の読み取りに使い、テストは、zip を、テストが使う小さな jar ファイルの作成に使う。筆者が zip を選んだのは、jar ファイルを読むには zip 形式の解析と DEFLATE の展開が必要であり、変更を行う者がそれを標準ライブラリだけで書くと、変更を行う者は mism の目的の外にある処理を実装することになり、zip が「依存するクレートの追加の規則」の 3 番目を満たすからである。変更を行う者は、zip の既定の機能を無効にし (`default-features = false`)、`deflate-flate2-zlib-rs` の機能だけを有効にする。jar ファイルが使う圧縮の方式は無圧縮と DEFLATE であり、既定の機能が加える暗号化と他の圧縮の方式を、mism は使わないからである。筆者は、データパックのファイル 3 つ (Terralith、veinminer、True Ending)と、Paper 26.3 と Folia 26.3 の `versions/` の下の jar ファイルが、無圧縮と DEFLATE だけを使うことを、2026 年 10 月 21 日に確認した。他の圧縮の方式を使うデータパックのファイルについて、mism は、`pack.mcmeta` を読めないことを伝えるエラーを報告する。筆者は、この機能の組み合わせで jar ファイルを書いて読めることを 2026 年 10 月 21 日に確認した。この組み合わせで zip が直接または間接に依存するクレートは、crc32fast、indexmap、memchr、typed-path、flate2、zlib-rs、cfg-if、equivalent、hashbrown である。筆者は、zip と同じ機能を持つ rc-zip を、rc-zip に依存している公開クレートの数が 12 であり「依存するクレートの追加の規則」の 3 番目を満たさないので退けた。
- mism は、serde-saphyr を、`plugin.yml` と `paper-plugin.yml` (YAML 形式)を型に読み込むために使う。serde-saphyr に依存している公開クレートの数は 420 なので、serde-saphyr は「依存するクレートの追加の規則」の 3 番目を満たさない。変更を行う者が YAML の構文解析を自分で実装すると、変更を行う者は、ブロックとフローの 2 つの書き方、引用符の規則、複数行の文字列、アンカーを含む YAML の仕様を実装することになり、その量が多すぎる。そのため、筆者は、「依存するクレートの追加の規則」の 5 番目に従い、3 番目を満たさない YAML のクレートの中で最も保守されているものとして serde-saphyr を選んだ。serde-saphyr には、2025 年 10 月 21 日から 2026 年 10 月 21 日までに 30 回のリリースがあり、直近 1 年以内にリリースがある YAML のクレートの中で、serde-saphyr に依存している公開クレートの数が最も多い。筆者は、serde-saphyr 2.0.0 が、文字列の型に読み込む値を YAML に書かれたとおりの文字列で返す (e.g. 引用符の無い `api-version: 1.20` を `1.20`、`version: 2.0` を `2.0`、`version: 010` を `010` として返す)こと、同じキーが 2 回あるマッピングをエラーにすること、先頭の BOM と CRLF の改行を受け付けることを、2026 年 10 月 21 日に確認した。変更を行う者は、serde-saphyr の既定の機能を無効にし、`deserialize` の機能だけを有効にする。この組み合わせで serde-saphyr が直接または間接に依存するクレートは、serde_core、granit-parser、annotate-snippets、num-traits、smallvec、encoding_rs_io、arraydeque、anstyle、unicode-width、encoding_rs、simdutf8、core_detect、multiversion_no_op、scopeguard、cfg-if である。筆者が退けた YAML のクレートと、その理由は次のとおりである。
  - serde_yaml は、作者が保守をやめ、最後のバージョンが `0.9.34+deprecated` である。
  - serde_yml は、RustSec の勧告 RUSTSEC-2025-0068 が、健全でなく保守されていないと報告している。
  - serde_yaml_ng は、serde_yaml_ng に依存している公開クレートの数が 911 であるが、最後のリリースが 2024 年 5 月 26 日であり、直近 1 年以内のリリースが無い。
  - serde_norway は、serde_norway に依存している公開クレートの数が 376 であり、最後のリリースが 2024 年 12 月 21 日である。
  - yaml-rust2 は、yaml-rust2 に依存している公開クレートの数が 233 であり、整数の形の値を数として読むので、`010` を `10` に、`0x1F` を `31` に変え、書かれた文字列を失う。筆者は、このことを yaml-rust2 0.13.0 で 2026 年 10 月 21 日に確認した。
  - saphyr は、saphyr に依存している公開クレートの数が 112 である。
- mism は、jiff を、Modrinth のバージョンの公開日時 (`date_published`)を時刻として解釈し、新旧を比べるために使う。Modrinth は公開日時を RFC 3339 の形の文字列で返すが、その文字列は、小数部の桁数と時差の書き方 (`Z` と `+00:00`)が一定でないので、mism は、文字列のままでは新旧を正しく比べられない。変更を行う者が RFC 3339 の構文解析を標準ライブラリだけで書くと、変更を行う者は mism の目的の外にある処理を実装することになるので、mism は、クレートを使う。筆者が jiff を選んだのは、jiff が「依存するクレートの追加の規則」の 3 番目を満たし、同じ機能を持つクレートの中で依存するクレートが最も少ないからである。既定の機能を無効にし `std` と `serde` の機能だけを有効にした jiff 0.2.38 が Linux で依存するクレートは、同じリポジトリで保守されている jiff-core と、mism が既に使う serde_core だけである。筆者は、jiff が `2026-10-06T07:14:55.653185Z`、`2021-01-05T13:18:36.857352+00:00`、`2026-10-06T07:14:55Z` を時刻として読み、`2026-10-06T07:14:55.653Z` を `2026-10-06T07:14:55.653185Z` より古いと判定することを、2026 年 10 月 21 日に確認した。筆者は、chrono を、既定の機能を無効にしても num-traits とそのビルドに使う autocfg の 2 つに依存するので退け、time を、deranged、num-conv、powerfmt、time-core の 4 つに依存するので退けた。
- mism は、`server.properties` の読み取りのためのクレートを足さない。Java の properties ファイルの形式を読むクレートのうち、java-properties は、java-properties に依存している公開クレートの数が 38 であり、最後のリリースが 2023 年 5 月 21 日なので、「依存するクレートの追加の規則」の 3 番目を満たさない。そのため、変更を行う者は、「依存するクレートの追加の規則」の 5 番目に従い、その形式の読み取りを自分で実装する (「server.properties の読み取り」の節)。その節の 5 つの規則を実装する量は、規則の 5 番目が挙げる TLS のように多すぎる量ではない。
- mism は、URL のパーセントエンコーディングのためのクレートを足さない。クエリ文字列の値は ureq がパーセントエンコーディングし (「HTTP クライアント」の節)、URL のパスに入れる Modrinth の識別子については、mism が `"` と `` ` `` の 2 文字だけを置き換えれば足りるからである (「Modrinth の API」の節)。
- mism は、signal-hook を、`SIGINT` と `SIGTERM` のハンドラの登録に使う。筆者が signal-hook を選んだのは、nix のハンドラを登録する関数 (`sigaction`)が `unsafe` であり、signal-hook は、フラグに値を記録するだけのハンドラを `unsafe` を書かずに登録できるからである。
- テストは、wiremock を、テストの中でローカルに立てる偽の HTTP サーバーに使う。筆者が wiremock を選んだのは、wiremock が、テストごとに HTTP サーバーを立てて決まったレスポンスを返す用途に作られているからである。
- テストは、tokio を、wiremock が必要とする非同期ランタイムとして使う。筆者が tokio を選んだのは、wiremock が tokio の上でだけ動くからである。mism の実行ファイルは、tokio に依存しない。
- テストは、tempfile を、各テストの一時ディレクトリの作成と、テストの終了時の削除に使う。筆者が tempfile を選んだのは、tempfile を使うと、テストを書く者が一時ディレクトリを削除する処理を各テストに書かずに済むからである。

筆者は、各クレートが `docs/development-rules.md` の「依存するクレートの追加の規則」の 3 番目 (そのクレートに依存しているクレートの数と直近のリリース)を満たすかを、2026 年 10 月 21 日に crates.io の API (`/api/v1/crates/<名前>` と `/api/v1/crates/<名前>/reverse_dependencies`)で確認した。次の表の「依存している公開クレートの数」は、crates.io がそのクレートに依存していると数える公開クレートの数である。

| クレート | 最新のバージョン | 最新のリリースの日 | 依存している公開クレートの数 | 3 番目の規則 |
|---|---|---|---|---|
| clap | 4.6.7 | 2026-09-14 | 49128 | 満たす |
| ureq | 3.4.2 | 2026-09-13 | 4390 | 満たす |
| serde | 1.0.229 | 2026-07-18 | 132445 | 満たす |
| serde_json | 1.0.154 | 2026-10-11 | 111925 | 満たす |
| toml | 1.1.8 | 2026-10-09 | 20579 | 満たす |
| sha2 | 0.11.1 | 2026-10-12 | 22205 | 満たす |
| thiserror | 2.0.21 | 2026-09-23 | 75206 | 満たす |
| nix | 0.31.3 | 2026-05-11 | 3862 | 満たす |
| signal-hook | 0.4.5 | 2026-10-04 | 1200 | 満たす |
| zip | 9.0.5 | 2026-10-17 | 4073 | 満たす |
| serde-saphyr | 2.0.0 | 2026-10-13 | 420 | 満たさない (依存している公開クレートの数) |
| jiff | 0.2.38 | 2026-10-06 | 1492 | 満たす |
| wiremock | 0.6.5 | 2025-08-24 | 3317 | 満たさない (直近 1 年以内のリリース) |
| tokio | 1.54.0 | 2026-10-19 | 78077 | 満たす |
| tempfile | 3.27.0 | 2026-03-11 | 37510 | 満たす |

退けたクレートについて、筆者が同じ日に確認した値は、chrono が 0.4.45 (2026-06-04、37928)、time が 0.3.55 (2026-08-01、6674)、rc-zip が 5.4.1 (2025-11-19、12)、yaml-rust2 が 0.13.0 (2026-09-11、233)、saphyr が 0.2.2 (2026-10-15、112)、serde_yaml_ng が 0.10.0 (2024-05-26、911)、serde_norway が 0.9.42 (2024-12-21、376)、serde_yml が 0.0.13 (2026-05-27、376)、serde_yaml が 0.9.34+deprecated (2024-03-25、7762)、java-properties が 2.0.0 (2023-05-21、38)である。括弧の中は、最新のリリースの日と、そのクレートに依存している公開クレートの数である。

## コマンドラインとエラーの出力

### 責務

`command_line.rs` は、コマンドラインの形を定め、`main.rs` は、サブコマンドの振り分け、エラーの出力、終了ステータスの決定を行う。
この 2 つのファイルは、仕様書の cli-1 から cli-5、err-9、new-10 の終了ステータス、ver-1 の終了ステータスを満たす。

### 方法

mism は、clap の derive マクロで、`new`、`versions`、`plugin`、`datapack` の 4 つのサブコマンドと、`plugin` と `datapack` のそれぞれの下の `search`、`versions`、`fetch`、`inspect`、`place`、`install`、`list`、`remove` の 8 つのサブコマンドと、それぞれの引数を定義する。mism は、`plugin` の下と `datapack` の下の 8 つのサブコマンドの引数と使い方の説明を、別々に定義する。2 組のサブコマンドの引数の形は同じである。mism が 2 組の引数と使い方の説明を別々に定義するのは、mism が、プラグインのサブコマンドの使い方の説明では jar ファイルとプラグインを、データパックのサブコマンドの使い方の説明ではデータパックのファイルとデータパックを指して書くからである。
mism は、利用者が <paper|folia> の位置に与えた引数を clap から文字列のまま受け取り、clap がコマンドラインを解析した後に、その文字列が `paper` と `folia` のどちらかであることを確かめる。どちらでもなければ、mism は、利用者が実行したサブコマンドの使い方を含む clap のエラーを標準エラー出力に出力し、終了ステータス 2 で終了する (cli-5)。
mism は、形式に合わない引数、`--help`、`--version` に対して、clap の既定の動作をそのまま使う (cli-1 から cli-4)。
mism は、`search` の `--limit` の値、<取得元の指定>、`versions` の引数、<プラグインの名前>、<データパックの名前> を clap から文字列のまま受け取り、各サブコマンドの手順がそれを解釈する (srch-5、dsrch-5、plg-3、dp-4、vers-4、dvers-4)。clap がこれらを解釈して形に合わない値を拒むと、clap が終了ステータス 2 で終了し、仕様書が定める終了ステータス 1 のエラーの報告 (err-9)にならないからである。

`main.rs` は、サブコマンドが返したエラーを、次の形で標準出力に出力し、終了ステータス 1 で終了する (err-9)。

```
mism: failed to create directory /home/sunset/servers/survival
mism: caused by: Permission denied (os error 13)
mism: hint: check that you have write permission to /home/sunset/servers
```

エラーの報告の最初の行は、mism が何をしようとして失敗したかを書いた行である。
`mism: caused by: ` で始まる各行は、`Error::source()` をたどって得た原因を、外側から順に 1 つずつ書いたものである。
`mism: hint: ` で始まる行は、利用者が取れる対処を書いた行であり、エラーの列挙型のバリアントのうち対処が決まっているものだけがこの行を持つ。
`main.rs` は、エラーの報告の各行を `output.rs` を通して書く (out-1)。
対処が決まっているのは、次の場合である。

- PaperMC が Minecraft のバージョンを配布していない場合 (err-3)、mism は、`mism versions` で配布しているバージョンを見るよう利用者に案内する。
- Java が見つからない場合と Java が古い場合、mism は、必要な Java のバージョンとインストールを利用者に案内する。
- 初回起動が失敗した場合 (err-7)、mism は、上に出力されたサーバーのログを見るよう利用者に案内する。
- 通信に失敗した場合 (err-5、srch-4、vers-5、dsrch-4、dvers-5)、mism は、ネットワークの接続を確認するよう利用者に案内する。
- カレントディレクトリに `mism.toml` が無い場合 (plg-1、dp-1)、mism は、`mism new` で作ったサーバーのディレクトリで実行するよう利用者に案内する。
- <取得元の指定> が 3 つの形のどれでもない場合 (plg-3、dp-4)、mism は、3 つの形を利用者に案内する。
- 識別子が Modrinth の識別子の文字の種類に合わない場合と、Modrinth にその識別子のプロジェクトが無い場合、mism は、利用者が実行したサブコマンドと同じ種類の `search` (`mism plugin search` か `mism datapack search`)で識別子を探すよう利用者に案内する。この節の以下の案内でも、mism は、利用者が実行したサブコマンドと同じ種類のサブコマンドを案内する。
- 利用者が与えた識別子と Modrinth が返した識別子が違う場合 (plg-3、dp-4)、mism は、Modrinth が返した識別子を使った取得元の指定を利用者に案内する。
- 候補のファイルを 1 つに絞れない場合 (plg-10、plg-11、dp-5)、mism は、候補が持つ値を示し、そのどれかの値を付けた取得元の指定を与えるよう利用者に案内する。
- Modrinth がリクエストの数の上限を理由にリクエストを断った場合 (HTTP ステータス 429)、mism は、1 分待ってからもう一度実行するよう利用者に案内する。
- ダウンロードした jar ファイルかデータパックのファイルの SHA-512 が Modrinth の公開する値と一致しない場合 (fetch-1、dfetch-1)、mism は、err-6 と同じく、もう一度実行するよう利用者に案内する。
- 取得元の指定のファイルがキャッシュのディレクトリに無い場合 (place-1、dplace-1)、mism は、`fetch` で取得し、`fetch` が出力した取得元の指定を与えるよう利用者に案内する。
- 取得元の指定のファイルがキャッシュのディレクトリに 2 つ以上ある場合、mism は、そのうちの 1 つのキャッシュのディレクトリの中のパスを取得元の指定として与えるよう利用者に案内する。
- 同じ名前の記録が `mism.toml` に既にある場合 (place-3、dplace-3)、mism は、`remove` でその記録を削除するよう利用者に案内する。同じ名前のものが置くディレクトリ (`plugins/` か `datapacks/`)に既にある場合 (place-4、dplace-4)、mism は、それを置くディレクトリの外に動かすよう利用者に案内する。
- 検証の判定に `conflict` がある場合 (place-6、inst-4)、mism は、`conflict` の判定が示した jar ファイルを `plugins/` の外に動かすよう利用者に案内する。
- `--lazy` で選べる Modrinth のバージョンが無い場合 (inst-2、dinst-2)、mism は、`versions` で照らし合わせた結果を見て、バージョンの番号を付けた取得元の指定を与えるよう利用者に案内する。
- 標準入力が端末でなく `--yes` も無い場合 (inst-7、dinst-7)、mism は、`--yes` を付けるよう利用者に案内する。
- 利用者が与えた名前の記録が `mism.toml` に無い場合 (rm-2、drm-2)、mism は、`list` で記録にある名前を見るよう利用者に案内する。
- キャッシュのディレクトリの中のファイルか `source.toml` が読めないか、ファイルの SHA-256 がそのディレクトリの名前と違う場合、mism は、そのディレクトリを削除してからもう一度取得するよう利用者に案内する。
- `plugins/` の jar ファイルの宣言が読めない場合、mism は、その jar ファイルを `plugins/` の外に動かすよう利用者に案内する。
- 対象のサーバーのディレクトリに `server.properties` が無い場合 (dp-3)、mism は、サーバーを 1 度起動して `server.properties` を作るよう利用者に案内する。`server.properties` が `level-name` を持たない場合と、`level-name` の値が空の場合 (dp-3)、mism は、`level-name` にワールドのディレクトリのパスを書くよう利用者に案内する。
- 対象のサーバーのディレクトリの `versions/` の下にサーバーの jar ファイルが無い場合 (「サーバーのデータパックの形式の番号」の節)、mism は、サーバーを 1 度起動してその jar ファイルを作るよう利用者に案内する。

変更を行う者は、エラーのメッセージを英語で書く。
mism は、中断による終了の終了ステータスを、`SIGINT` のとき 130、`SIGTERM` のとき 143 にする。

### 性質

- d-cli-1. mism がエラーを報告したとき、エラーの報告の最初の行は `mism: ` で始まる。標準出力の中で、サーバーの出力 (new-5)がエラーの報告の前にあることはあるが、サーバーの出力の行が `mism: ` で始まることはない。
- d-cli-2. エラーに原因のエラーがあるとき、標準出力には、`Error::source()` をたどって得られる原因のエラーの数と同じ数だけ、`mism: caused by: ` で始まる行がある。

### 判断

- 変更を行う者がエラーのメッセージを英語で書くのは、サーバーのログと Java のメッセージが英語であり、利用者が 1 つの端末で読む文の言語を揃えるためである。
- mism が終了ステータスを `SIGINT` のとき 130 に、`SIGTERM` のとき 143 にするのは、シェルの慣習 (128 にシグナルの番号を足す)に合わせるためである。
- mism が `paper` と `folia` を clap の列挙型 (`ValueEnum`)として定義しないのは、clap 4.6 が、列挙型に無い値のエラーに使い方を含めないからである。筆者は、このことを 2026 年 10 月 20 日に確認した。

## mism 自身の出力

### 責務

`output.rs` は、mism 自身が標準出力に書く文字列の各行の先頭に `mism: ` を付けること、外部のソフトウェアの出力の始まりと終わりを伝える文字列、警告の文字列、標準出力が端末のときの色を担う。mism 自身は、標準エラー出力に何も書かない (out-4)。
このモジュールは、仕様書の out-1 から out-3、out-5、plg-9、dp-12 を満たし、new-10、ver-1、err-9 の出力の形を決める。

### 方法

mism は、mism 自身が書く文字列の各行の先頭に `mism: ` を付ける (out-1)。
`subcommands/new.rs` の結果の記述 (new-10)、`subcommands/versions.rs` の一覧の各記述 (ver-1)、`subcommands/plugin/` と `subcommands/datapack/` が書くすべての文字列、`prompt.rs` の問い、`main.rs` のエラーの報告と中断の報告 (err-9)は、すべてこのモジュールを通して書く。

このモジュールは、`mism plugin` と `mism datapack` のサブコマンドが並べる値から、値をタブ 1 文字で区切り、一覧の値の要素を `,` で区切り、無い値と要素が 1 つも無い一覧を `-` にした、改行を含まない 1 つの文字列を作る (plg-9、dp-12)。
問いは、`prompt.rs` が標準入力から回答を読む直前に書く文字列である。このモジュールは、問いの先頭に `mism: ` を付け、問いの後に改行を書かず、書いた直後にフラッシュする。利用者が回答を問いの直後に続けて入力できるようにするためである。

サーバーの出力の始まりを伝える文字列は `mism: output of <jar ファイルの名前> follows` であり、サーバーの出力の終わりを伝える文字列は `mism: output of <jar ファイルの名前> ended` である (out-2)。
`server_directory/initial_launch.rs` は、サーバーのプロセスを起動する直前に始まりを伝える文字列と空行 1 行を、サーバーのプロセスが終了した直後 (中断でサーバーを止めた後を含む)に空行 1 行と終わりを伝える文字列を、このモジュールを通して標準出力に書く。

mism は、標準出力が端末 (`isatty`)であるときだけ、このモジュールを通して書く各行 (結果の記述、エラーの報告、問い、サーバーの出力の始まりと終わりを伝える 2 つの文字列を含む)の先頭の `mism:` を、ANSI の SGR の前景色の青 (パラメーター 34。`ESC[34m` から `ESC[0m` まで)で書く (out-3)。
その青の実際の色合いは、端末のテーマが決める。
警告の文字列は、警告の本文の各行の先頭に `mism: warning: ` を付けたものである (out-5)。mism は、標準出力が端末であるときだけ、警告の各行の先頭の `mism:` を、ANSI の SGR の前景色の黄 (パラメーター 33。`ESC[33m` から `ESC[0m` まで)で書く (out-5)。`subcommands/` のモジュールは、警告を、このモジュールの警告を書く処理を通して書く。
mism は、mism が標準出力に書く各行の先頭の `mism:` より後の部分には色を付けず、標準出力が端末でないときはエスケープシーケンスを一切書かない。
mism は、標準出力に書くたびに、書いた内容をフラッシュする。サーバーが mism と同じ標準出力に書くので、フラッシュしなければ mism の文字列とサーバーの出力の順序が保たれないからである。

### 判断

- 筆者が青を RGB ではなく ANSI の SGR のパラメーター 34 で指定するのは、リポジトリの所有者が、色を固定せず端末のテーマに任せると決めたからである。
- 筆者が色を、mism が標準出力に書く各行の先頭の `mism:` だけに付け、その色を、警告の行では黄、それ以外の行では青にするのは、仕様書の out-3 と out-5 がそれだけを定めているからである。筆者が黄を RGB ではなく SGR のパラメーター 33 で指定するのは、筆者が青を SGR のパラメーター 34 で指定するのと同じく、筆者が色合いを端末のテーマに任せるためである。
- 警告の各行が `warning: ` を持つのは、標準出力が端末でなく mism が色を付けないときにも、利用者が、警告の行を、mism が標準出力に書くほかの文字列と見分けられるようにするためである (out-5)。
- 筆者が始まりと終わりを伝える文字列を `initial_launch.rs` に書かせるのは、jar ファイルの名前と、サーバーのプロセスの起動と終了の時点を、そのモジュールだけが持つからである。
- 筆者が `mism new` の結果の文字列 (new-10)と Minecraft のバージョンの一覧の各文字列 (ver-1)にも `mism: ` を付けるのは、リポジトリの所有者が、機械向けの出力は将来のオプション (`--json`)で分け、それ以外の出力をすべて人向けにすると決めたからである。

## HTTP クライアント

### 責務

`http_client.rs` は、JSON の取得と jar ファイルのストリームでの受信を行い、User-Agent ヘッダーとタイムアウトを決める。
このモジュールは、仕様書の err-5、srch-4、vers-5、dsrch-4、dvers-5 の検出と、fetch-1、fetch-2、dfetch-1、dfetch-2 のダウンロードを担う。この節の jar ファイルのダウンロードについての決定は、データパックのファイルのダウンロード (dfetch-1、dfetch-2)にも当てはまる。

### 方法

mism は、すべてのリクエストに `mism/<mism のバージョン> (me@5unset.me)` という User-Agent ヘッダーを付ける。

JSON を取得するとき、このモジュールは、呼び出し側からクエリ文字列の名前と値の組を受け取り、ureq の機能でそれぞれをパーセントエンコーディングして URL に付ける。呼び出し側は、クエリ文字列を URL の文字列に自分でつながない。
mism は、ureq の既定のとおり、リダイレクトを 10 回までたどる。利用者が与えた jar ファイルの URL (fetch-2)は、配布元の別のホストへリダイレクトすることがあるからである。

mism は、接続の確立に 10 秒を超えた場合と、JSON のレスポンスの受信に合計で 30 秒を超えた場合に、通信が失敗したとみなす。
mism は、jar ファイルのダウンロードには合計の時間の上限を設けず、データが 30 秒間まったく届かない場合にだけ失敗とみなす。
テストは、この 30 秒を環境変数 `MISM_DOWNLOAD_IDLE_TIMEOUT` で上書きできる。

mism は、HTTP ステータスが 200 ではないレスポンスを、取得の失敗として扱う。
取得の失敗を伝えるエラーは、リクエストした URL と HTTP ステータスを含め、レスポンスの JSON が `message` フィールド (PaperMC の形)か `description` フィールド (Modrinth の形)を持つ場合はその文字列も含める。呼び出し側は、そのエラーから HTTP ステータスを取り出せる (「Modrinth の API」の節)。

mism は、jar ファイルのダウンロードの間、ストリームからまとまったバイト列を 1 回読むたびに中断のフラグを確認し、フラグが立っていればダウンロードをやめる。

### 性質

- d-http-1. jar ファイルのダウンロード中にデータが届かない状態が 30 秒 (または `MISM_DOWNLOAD_IDLE_TIMEOUT` の秒数)続いたとき、mism は、通信の失敗としてダウンロードを中止してエラーを報告する。
- d-http-2. jar ファイルのダウンロードが遅くても進んでいるとき、mism は、時間を理由にダウンロードを打ち切らない。
- d-http-3. クエリ文字列の値が `&`、`=`、`#`、空白、`"` を含むとき、偽のサーバーが受け取るそのクエリ文字列の値は、mism に与えた値と同じである。

### 判断

- mism がこの User-Agent ヘッダーを付けるのは、PaperMC のドキュメントが、ソフトウェアの名前と連絡先を含む User-Agent ヘッダーを求めているからである。Modrinth の API も、API を呼ぶソフトウェアを識別できる User-Agent ヘッダーを求めている。筆者は、Modrinth のドキュメントのこの記述を 2026 年 10 月 20 日に確認した。
- mism がクエリ文字列のパーセントエンコーディングを ureq に任せるのは、Modrinth の検索の語 (srch-1)が利用者の入力であり、`&` や `#` を含むと、mism が文字列をつないで作った URL では別のクエリ文字列になるからである。筆者は、ureq 3.4.2 の `RequestBuilder::query` が名前と値をパーセントエンコーディングすることを、2026 年 10 月 21 日にソースコードで確認した。
- 筆者は、ureq 3.4.2 がリダイレクトを既定で 10 回までたどることを、2026 年 10 月 21 日にソースコードで確認した。
- 筆者は、Modrinth の API がエラーのときに `error` と `description` の 2 つのキーを持つ JSON を返し、存在しないプロジェクトに対しては HTTP ステータス 404 と空の本文を返すことを、2026 年 10 月 21 日に確認した。
- mism がダウンロードに合計の時間の上限を設けないのは、回線が遅いだけで進んでいるダウンロードを打ち切らないためである (e.g. mism が 55 MB の jar ファイルを 80 KB/s で受信すると、受信に約 11 分かかる)。mism が止まった接続だけを検出するには、mism がデータの届かない時間で判定すれば足りる。
- 筆者は、ureq 3.4.2 の設定に、データが届かない時間によるタイムアウトが無いことを、2026 年 10 月 20 日に確認した。設定にあるのは、接続、ヘッダーの受信、本文の受信のそれぞれにかかる合計の時間の上限だけである。そのため、mism は、ureq の `unversioned::transport` モジュールの API で接続をラップし、ソケットから 1 回読むたびの待ち時間に上限を付けて、データが届かない時間によるタイムアウトを実現する。このモジュールの API は semver の対象外なので、変更を行う者は、Cargo.toml で ureq のバージョンをマイナーバージョンまで固定する。
- mism が HTTPS でダウンロードしている間に `SIGINT` または `SIGTERM` を受けたとき、データが届いていれば、mism は、次にデータを読んだ時点で中断に気付く。データが届いていなければ、rustls が、シグナルで中断された読み取り (`EINTR`)を内部でやり直すので、mism は、データが届かない時間の上限 (30 秒)が過ぎた時点で中断に気付く。筆者は、このことを 2026 年 10 月 20 日に確認した。

## Minecraft のバージョンの新旧の判定

### 責務

`minecraft_version.rs` は、Minecraft のバージョンを解析し、2 つの Minecraft のバージョンのどちらが新しくリリースされたかを決め、Minecraft のバージョンの一覧の中で最も新しいものを決める。
このモジュールは、仕様書の sel-1 と ver-1 の「リリースが新しい順」の判定と、srch-2、vers-2、dsrch-2、dvers-2 の「Minecraft のバージョンのうち最も新しいもの」の判定と、insp-3 の「最小の Minecraft のバージョンが対象のサーバーの Minecraft のバージョンより新しい」かの判定を担う。

### 方法

PaperMC の API は、Minecraft のバージョンを PaperMC に登録された日時の新しい順に返すが、API のドキュメントはその順序を保証していない。
そのため、mism は、どちらが新しくリリースされたかを、Minecraft のバージョンの解析によって決める。

mism は、Minecraft のバージョンを、数の並びと接尾辞に分けて解析する。
数の並びは、1 つ以上の非負整数を `.` でつないだものである (e.g. `26.3` の数の並びは `[26, 3]`、`1.21.11` の数の並びは `[1, 21, 11]`)。
接尾辞は、次の 4 つのどれかである。

- 接尾辞が無い Minecraft のバージョン (e.g. `26.3`、`26.1.2`、`1.21.11`)は、正式リリースを表す。
- `-snapshot<数>` または `-snapshot-<数>` の接尾辞を持つ Minecraft のバージョン (e.g. `26.4-snapshot-5`)は、スナップショットを表す。
- `-pre<数>` または `-pre-<数>` の接尾辞を持つ Minecraft のバージョン (e.g. `1.21.11-pre5`、`26.1-pre-1`)は、プレリリースを表す。
- `-rc<数>` または `-rc-<数>` の接尾辞を持つ Minecraft のバージョン (e.g. `1.21.11-rc3`、`26.3-rc-3`)は、リリース候補を表す。

mism は、2 つの Minecraft のバージョンを次の順で決着が付くまで比べる。

1. mism は、数の並びを先頭から 1 つずつ整数として比べる。数の並びの長さが違うときは、mism は、短い方の足りない部分を 0 とみなす。
2. 数の並びが等しければ、mism は、接尾辞の種類で比べ、スナップショット、プレリリース、リリース候補、正式リリースの順に新しいとみなす。
3. 接尾辞の種類も等しければ、mism は、接尾辞の数が大きい方を新しいとみなす。

どの形にも当てはまらない文字列を解析したとき、このモジュールは、その文字列を含むエラーを返す。
そのエラーを受け取った呼び出し側の扱いは、文字列の出どころによって次のとおり違う。

- PaperMC の API が返した Minecraft のバージョン (sel-1、ver-1)と、jar ファイルの宣言の `api-version` (insp-3)について、mism は、そのエラーを報告する。
- Modrinth が申告する Minecraft のバージョン (Modrinth のバージョンの `game_versions` の要素)について、mism は、解析できない値 (e.g. `25w46a`、`b1.7.3`)を新旧の比較から除く (「検証の値と判定」の節)。

mism は、Modrinth が申告する Minecraft のバージョンの一覧のうち最も新しいもの (srch-2、vers-2、dsrch-2、dvers-2)を、次のとおり決める。mism は、一覧の各要素を `minecraft_version.rs` で解析し、解析できない要素を除き、残りの中で最も新しいものを選ぶ。最も新しいものが複数あれば (e.g. `1.21` と `1.21.0`)、mism は、一覧で先にあるものを選ぶ。解析できる要素が 1 つも無ければ、最も新しいものは無く、mism は `-` を出力する (plg-9)。

### 性質

- d-ver-1. mism が `26.1` と `26.1.2` を比べたとき、`26.1.2` が新しい。
- d-ver-2. mism が `26.3-rc-3` と `26.3` を比べたとき、`26.3` が新しい。
- d-ver-3. mism が `1.21.11-pre5` と `1.21.11-pre6` を比べたとき、`1.21.11-pre6` が新しい。
- d-ver-4. mism が `1.21.11-rc3`、`1.21.11`、`26.1.2`、`26.3-rc-3`、`26.3` を並べたとき、この順が古い順である。
- d-ver-5. mism が 4 つの形のどれにも当てはまらない文字列 (e.g. `26.3-snapshot`)を解析したとき、mism は、その文字列を含むエラーを報告する。
- d-ver-6. mism が `26.3-snapshot-10`、`26.3-pre-1`、`26.3-rc-1`、`26.3` を並べたとき、この順が古い順である。
- d-ver-7. mism が `26.3-snapshot-9` と `26.3-snapshot-10` を比べたとき、`26.3-snapshot-10` が新しい。
- d-compat-5. Modrinth が申告する Minecraft のバージョンの一覧が `26.3`、`26.4-snapshot-5`、`25w46a` であるとき、その中で最も新しいものは `26.4-snapshot-5` である。
- d-compat-6. Modrinth が申告する Minecraft のバージョンの一覧のどの要素も解析できないとき、mism は、最も新しい Minecraft のバージョンとして `-` を出力する。

### 判断

- 筆者は、正式リリース、プレリリース、リリース候補の 3 つの形を、2026 年 10 月 20 日に API が返した実際の Minecraft のバージョンから取った。Minecraft は 2026 年のリリースからバージョンの形を変えたので、PaperMC は `-rc3` と `-rc-3` のような 2 つの形を両方とも配布している。
- mism が接尾辞の順序をスナップショット、プレリリース、リリース候補、正式リリースとするのは、Minecraft がこの順に出すからである。
- 筆者は、Modrinth の Minecraft のバージョンの一覧 (`GET /v2/tag/game_version`)が、2026 年からの形のスナップショット (e.g. `26.4-snapshot-5`)と、それより前の形のスナップショット (e.g. `25w46a`、`24w33a`)と、さらに古い形 (e.g. `b1.7.3`、`rd-132211`)を含み、`26.3-snapshot-1` から `26.3-snapshot-10`、`26.3-pre-1` から `26.3-pre-3`、`26.3-rc-1` から `26.3-rc-3`、`26.3` の順に公開日時が新しいことを、2026 年 10 月 21 日に確認した。mism がスナップショットの形を解析するのは、2026 年からの形のスナップショットが、数の並びによって正式リリースとの新旧を決められるからである。それより前の形のスナップショットとさらに古い形は、文字列から正式リリースとの新旧が決まらないので、mism は解析しない。
- mism が Modrinth の申告する解析できない値を新旧の比較から除き、エラーにしないのは、それらの値が Modrinth の Minecraft のバージョンの一覧に正しく載っている値であり、想定と違う状態ではないからである。PaperMC はそれらの Minecraft のバージョンの Paper を配布しないので、それらの値が対象のサーバーの Minecraft のバージョンになることは無い。

## PaperMC の API

### 責務

`papermc_api/` は、PaperMC の API を呼び、Minecraft のバージョンと Paper (または Folia)のバージョンを選ぶ。
`papermc.rs` と `folia.rs` は、API のパスに入れるプロジェクトの名前 (`paper`、`folia`)を持つ。Paper にだけ必要な処理と Folia にだけ必要な処理は、今は無い。
これらのモジュールは、仕様書の sel-1 から sel-3、err-3、err-4、err-5、ver-1、ver-2 を満たす。

### 方法

mism は、PaperMC がダウンロードのために提供している API のバージョン 3 (ベース URL `https://fill.papermc.io/v3`)を使う。
環境変数 `MISM_PAPERMC_URL` が設定されているとき、mism は、その値をベース URL として使う。

PaperMC の API は、Paper (または Folia)のバージョンを `build` という名前で呼ぶ。
mism は、その名前を URL と JSON のフィールド名としてそのまま使うが、変更を行う者は、コードと文書の中ではそれを Paper (または Folia)のバージョンと呼ぶ。

mism は、次の 2 つのエンドポイントだけを使う。

バージョンの一覧のエンドポイントは、`GET /v3/projects/{paper|folia}/versions` である。
mism は、このレスポンスの `versions` の各要素から、`version.id` (Minecraft のバージョン)、`version.support.status` (サポート状態)、`builds` (Paper (または Folia)のバージョンの配列)、`version.java.version.minimum` (必要な Java の最小のバージョン)を読む。

ダウンロードの情報のエンドポイントは、`GET /v3/projects/{paper|folia}/versions/{Minecraft のバージョン}/builds/{Paper (または Folia)のバージョン}` である。
mism は、このレスポンスの `downloads` の `server:default` が持つ `name` (jar ファイルの名前)、`checksums.sha256`、`url` を読む。
mism は、jar ファイルを `url` が示す URL からダウンロードし、jar ファイルの URL を文字列をつないで作ることはしない。

`papermc_api/mod.rs` は、API を呼ぶ処理をトレイトとして定義する。
トレイトは、「Minecraft のバージョンの一覧を得る」と「1 つの Paper (または Folia)のバージョンのダウンロードの情報を得る」の 2 つの操作を持つ。
HTTP で呼ぶ実装は、`http_client.rs` を使う。

`papermc_api/selection.rs` は、Minecraft のバージョンと Paper (または Folia)のバージョンを、次の手順で選ぶ。

利用者が `--version` を指定した場合、mism は、次の手順で選ぶ。

1. mism は、バージョンの一覧のエンドポイントから、Minecraft のバージョンの一覧を得る。
2. 一覧の中に `version.id` が利用者の指定した文字列と一致する要素が無ければ、mism は、サーバーの作成を中止してエラーを報告する (err-3)。mism は、一覧と照合した後の文字列だけを URL に入れる。
3. mism は、その要素の `builds` の最大値を Paper (または Folia)のバージョンとする (sel-3)。

利用者が `--version` を省略した場合、mism は、次の手順で選ぶ。

1. mism は、バージョンの一覧のエンドポイントから得た一覧を、`version.id` のリリースが新しい順に並べる。
2. mism は、並べた順に見て、`version.support.status` が `SUPPORTED` である最初の要素を選ぶ (sel-1)。1 つも無ければ、mism は、サーバーの作成を中止してエラーを報告する (err-4)。
3. mism は、その要素の `builds` の最大値を Paper (または Folia)のバージョンとする (sel-3)。

mism が選んだ要素の `builds` が空の配列である場合、mism は、その Minecraft のバージョンの Paper (または Folia)が配布されていないことを伝えるエラーを報告する。

どちらの場合も、その後に mism は、ダウンロードの情報のエンドポイントから jar ファイルの名前、SHA-256、URL を得る。

### 性質

- d-api-1. `mism new` は、API を 2 回だけ呼ぶ。
- d-api-2. mism が URL に入れる Minecraft のバージョンの文字列は、バージョンの一覧のエンドポイントが返した `version.id` のどれかである。
- d-api-3. バージョンの一覧のエンドポイントが返した順序と、リリースが新しい順が食い違うとき、mism は、リリースが新しい順に従って選ぶ。

### 判断

- 筆者は、バージョンの一覧のエンドポイントがすべての Minecraft のバージョンとそのサポート状態を 1 回で返すことを、2026 年 10 月 20 日に確認した。筆者は、このレスポンスの各要素が持つキーが `builds`、`version.id`、`version.support.status`、`version.support.end`、`version.java.version.minimum`、`version.java.flags.recommended` であることも、同じ日に確認した。
- 筆者は、ダウンロードの情報のエンドポイントが存在し、そのレスポンスが `id`、`channel`、`time`、`downloads` を持ち、`downloads` の `server:default` が `name`、`checksums.sha256`、`url` を持つことを、Paper (`26.3` の `206`)と Folia (`26.3` の `3`)について 2026 年 10 月 20 日に確認した。存在しない Minecraft のバージョンや Paper (または Folia)のバージョンを URL に入れると、この API は、HTTP ステータス 404 と、`ok`、`error`、`message` の 3 つのキーを持つ JSON を返す。筆者は、この応答を同じ日に確認した。
- 旧バージョンの API (`https://api.papermc.io/v2`)は、2026 年 7 月 1 日に廃止されており、HTTP ステータス 410 を返す。筆者は、この HTTP ステータスを 2026 年 10 月 20 日に確認した。
- 筆者が API を呼ぶ処理をトレイトにするのは、Minecraft のバージョンと Paper (または Folia)のバージョンを選ぶ手順を、通信なしで単体テストするためである。
- 筆者は、`builds` が空の配列になり得るかを、確認していない。2026 年 10 月 20 日にバージョンの一覧のエンドポイントが返した Paper の 68 個と Folia の 13 個の Minecraft のバージョンには、`builds` が空のものは無かった。仕様書には、この場合の番号付きの文が無い。

## 書き込みが終わるまで名前を持たないファイルの操作

### 責務

`atomic_fs.rs` は、書き込みが終わるまで名前を持たないファイルの作成、既存の名前を上書きしない名前の変更、既にあるファイルの置き換え、作業用の名前の生成を提供する。
このモジュールは、`jar_cache.rs`、`fetched_file_cache.rs`、`server_directory/creation.rs`、`server_directory/mism_toml.rs`、`server_directory/placed_files.rs` が使い、plg-8 と dp-11 の「`mism.toml` が実行の前と同じ」を支える。

### 方法

mism は、名前を持たないファイルを、置く予定のディレクトリの中に `O_TMPFILE` で作る。
このファイルは、名前を付けるまでディレクトリの一覧に現れず、mism がどのように終了してもカーネルが解放する。
mism は、書き込みが終わったファイルに `linkat` で名前を付ける。
`O_TMPFILE` に対応していないファイルシステムでは、mism は、ファイルの作成を中止してエラーを報告する。

mism は、ディレクトリとファイルの名前の変更に `renameat2` の `RENAME_NOREPLACE` を使い、変更先に既に何かあれば失敗させる。

作業用の名前は、`.mism-tmp-<16 桁のランダムな 16 進数>` である。このモジュールは、16 桁の 16 進数を、`/dev/urandom` から読んだ 8 バイトから作る。`server_directory/creation.rs` の作業用のディレクトリ (「mism new の手順と中断」の節)、置き換えの途中のファイル、`fetched_file_cache.rs` の作業用のディレクトリ、`server_directory/placed_files.rs` が削除の途中でファイルに付ける名前は、すべてこの形の名前を使う。

mism は、既にあるファイルを新しい内容に置き換えるとき、次の手順で置き換える。置き換える先にファイルが無いときも、mism は同じ手順で書く。

1. mism は、置き換える先と同じディレクトリに、名前を持たないファイルを作り、新しい内容を書く。
2. mism は、そのファイルに、同じディレクトリの中の作業用の名前を `linkat` で付ける。
3. mism は、`rename` で、作業用の名前を置き換える先の名前に変える。`rename` は、置き換える先に既にあるファイルを、他のプロセスから見て一度に置き換える。
4. 手順 2 か手順 3 が失敗したとき、mism は、作業用の名前のファイルがあればそれを削除し、エラーを報告する。

### 性質

- d-fs-1. 名前を付ける操作の前に mism がどのように終了しても、ディレクトリに名前を持つファイルは増えない。
- d-fs-2. 名前を付ける操作の前後で、他のプロセスから見ると、名前を付ける先のパスのファイルは「無い」か「書き込みが終わっている」のどちらかにしか見えない。
- d-fs-3. 置き換えの手順 3 が終わるまで、置き換える先のパスのファイルは古い内容のままであり、手順 3 が終わった後は新しい内容である。
- d-fs-4. 置き換えが成功したとき、そのディレクトリには、作業用の名前のファイルが残っていない。

### 判断

- mism が、`O_TMPFILE` に対応していないファイルシステムで、名前を持つ一時ファイルを作ってから名前を変えるような別の方法に切り替えないのは、開発のルールのフォールバックをしないという規則による。ext4、XFS、Btrfs、tmpfs は `O_TMPFILE` に対応している。
- mism が置き換えで作業用の名前を一度付けるのは、`linkat` が既にある名前を置き換えられず、既にある名前を一度に置き換えられるのが `rename` だけだからである。これは `O_TMPFILE` が使えないときの別の方法ではなく、置き換えの唯一の方法である。置き換えの手順 2 と手順 3 の間で mism が `SIGKILL` で止められると、作業用の名前のファイルが残る。置き換える先のファイルは、その場合も古い内容のままである。
- 作業用の名前が `.mism-tmp-` で始まり `.jar` と `.zip` のどちらでも終わらないのは、利用者がその名前を見てそのファイルを mism が残したものと分かり、サーバーが `plugins/` と `datapacks/` に残ったそのファイルをプラグインやデータパックとして読み込まないようにするためである。筆者は、Paper が `plugins/` の中の名前が `.jar` で終わるファイルだけをプラグインとして読み込むことと、`datapacks/` の中の名前が `.zip` で終わるファイルとディレクトリだけをデータパックとして読み込むことを、確認していない。
- 筆者は、`renameat2` の `RENAME_NOREPLACE` が使えるカーネルのバージョンを、確認していない。
- Rust のクレートの nix 0.31 は、`renameat2` を glibc 向けにだけ提供しているので、変更を行う者は、mism を musl 向けにはビルドできない。筆者は、このことを 2026 年 10 月 20 日に確認した。

## キャッシュのディレクトリとハッシュの計算

### 責務

`cache_directory.rs` は、キャッシュのディレクトリのパスを決める。`content_hash.rs` は、jar ファイルの SHA-256 と SHA-512 を計算する。
`jar_cache.rs` と `fetched_file_cache.rs` がこの 2 つのモジュールを使い、`server_directory/placed_files.rs` が `content_hash.rs` を使う。
`cache_directory.rs` は、仕様書の cache-1 と cache-2 を満たす。

### 方法

キャッシュのディレクトリは、環境変数 `XDG_CACHE_HOME` が設定されていて空でなければその中の `mism`、そうでなければ `~/.cache/mism` である (cache-1、cache-2)。`XDG_CACHE_HOME` も `HOME` も設定されていなければ、mism は、キャッシュのディレクトリが決まらないことを伝えるエラーを報告する。
Paper (または Folia)の jar ファイルは `<キャッシュのディレクトリ>/<paper|folia>/` の下に、プラグインの jar ファイルは `<キャッシュのディレクトリ>/plugins/` の下に、データパックのファイルは `<キャッシュのディレクトリ>/datapacks/` の下に置かれる。

`content_hash.rs` は、受け取った内容を別のファイルに書きながら SHA-256 (と、呼び出し側が求めれば SHA-512)を計算する処理と、既にあるファイルを読みながら SHA-256 を計算する処理を持つ。どちらも、結果を小文字の 16 進数の文字列にする。

### 判断

- 筆者がキャッシュのディレクトリの決定とハッシュの計算を `jar_cache.rs` から切り出すのは、`fetched_file_cache.rs` が同じキャッシュのディレクトリを使い、`fetched_file_cache.rs` と `server_directory/placed_files.rs` が同じハッシュの計算を使うからである。

## jar ファイルのキャッシュ

### 責務

`jar_cache.rs` は、mism が選んだ Paper (または Folia)のバージョンの jar ファイルを、キャッシュのディレクトリから取り出すか、ダウンロードしてキャッシュのディレクトリに保存する。
このモジュールは、仕様書の cache-3 から cache-6 と err-6 を満たす。キャッシュのディレクトリの場所 (cache-1、cache-2)は、`cache_directory.rs` が決める。

### 方法

キャッシュのディレクトリの中の jar ファイルのパスは、`<キャッシュのディレクトリ>/<paper|folia>/<Minecraft のバージョン>/<Paper (または Folia)のバージョン>/<jar ファイルの名前>` である (e.g. `~/.cache/mism/paper/26.3/206/paper-26.3-206.jar`)。

1. そのパスにファイルがあれば、mism は、それを読みながら SHA-256 を計算し、計算した SHA-256 が API の値と一致すればそのファイルをそのまま使う (cache-3)。計算した SHA-256 が一致しなければ、mism は、手順 2 に進む (cache-4)。
2. mism は、同じディレクトリの中に、名前を持たないファイルを作る。
3. mism は、受信した内容をそのファイルに書きながら SHA-256 を計算する。
4. 計算した SHA-256 が API の値と一致しなければ、mism は、そのファイルを閉じ、サーバーの作成を中止してエラーを報告する (err-6)。
5. 一致すれば、mism は、手順 1 で一致しなかったファイルがあればそれを削除し、名前を持たないファイルに、この節の冒頭で定めた jar ファイルのパスの名前を付ける。
6. 手順 5 で「その名前は既にある」という理由で名前を付けられなかった場合、mism は、mism が作った名前を持たないファイルを閉じ、既にあるファイルを手順 1 と同じ方法で照合して使う。

mism は、キャッシュのディレクトリにある jar ファイルを、利用者の操作なしには削除しない (cache-6)。

### 性質

- d-cache-1. ダウンロードの途中で mism がどのように終了しても、キャッシュのディレクトリに名前を持つファイルは増えない。
- d-cache-2. キャッシュのディレクトリの中の jar ファイルのパスにファイルが存在するとき、そのファイルは、書き込みが終わり、SHA-256 の照合を 1 度は通っている。
- d-cache-3. 同じ jar ファイルを 2 つの mism が同時にダウンロードしたとき、両方が成功し、そのパスにはファイルが 1 つだけ存在する。

### 判断

- mism が jar ファイルの内容を名前を持たないファイルに書くのは、途中で止まったファイルがそのパスに残ると、次回の mism がそれを完成したファイルとして読んでしまうからである。
- mism が名前を持たないファイルをキャッシュのディレクトリと同じディレクトリに作るのは、名前を付ける操作が同じファイルシステムの中でしか成り立たないからである。
- mism が名前を付ける前に SHA-256 を API の値と照合するので、d-cache-2 が成り立つ。これは、キャッシュのディレクトリから読む側が照合する cache-3 とは別の保証である。

## mism new の手順と中断

### 責務

`subcommands/new.rs` は、`mism new` の手順の全体を担い、`server_directory/creation.rs` は、利用者が指定した <パス> の検査、作業用のディレクトリの作成、<パス> への配置を担う。
利用者が mism を中断したとき、mism は、起動中のサーバーを止め、<パス> に何も残さない。
この 2 つのモジュールは、仕様書の new-1、new-2、new-10、err-1、err-2、err-8、int-1 を満たす。

### 方法

mism は、サーバーの準備を、<パス> の隣に作った作業用のディレクトリの中で行い、すべて終わってからそのディレクトリの名前を <パス> に変える。

1. <パス> が相対パスなら、mism は、<パス> の前にカレントディレクトリのパスをつないで、<パス> を絶対パスにする (new-1)。
2. mism は、<パス> をシンボリックリンクをたどらずに調べ (`lstat`)、ファイル、ディレクトリ、シンボリックリンクのどれかがあれば、サーバーの作成を中止してエラーを報告する (err-1)。
3. <パス> の親ディレクトリが無い場合と、親ディレクトリのパスにディレクトリ以外のものがある場合、mism は、サーバーの作成を中止してエラーを報告する (err-2)。
4. mism は、Minecraft のバージョンと Paper (または Folia)のバージョンを選び、jar ファイルをキャッシュのディレクトリに用意する。
5. mism は、親ディレクトリの中に `.mism-tmp-<16 桁のランダムな 16 進数>` という名前の作業用のディレクトリを作る。
6. mism は、キャッシュのディレクトリの jar ファイルを作業用のディレクトリに複製し、複製した内容の SHA-256 を、ダウンロードの情報のエンドポイントが返した SHA-256 と照合する。
7. mism は、作業用のディレクトリの中でサーバーを初回起動し、`eula.txt` を書き換え、`mism.toml` を作る (「初回起動」の節)。
8. mism は、作業用のディレクトリの名前を <パス> に変える。<パス> に既に何かあれば名前の変更は失敗し、mism は、サーバーの作成を中止してエラーを報告する。
9. mism は、`paper` か `folia`、Minecraft のバージョン、Paper (または Folia)のバージョン、<パス> をこの順に空白 1 文字で区切った記述を、`output.rs` を通して (先頭に `mism: ` を付けて) 1 行として標準出力に書く (new-10、out-1)。

手順 5 から 8 のどこかで mism がエラーを報告する場合、mism は、作業用のディレクトリを中身ごと削除する (err-8)。

mism は、中断を次のとおり扱う (int-1)。

- mism は、サーバーのプロセスを mism とは別のプロセスグループで起動する。
- mism は、サーバーの終了を待つ間、100 ミリ秒ごとに中断のフラグを確認する。フラグが立っていたら、mism は、サーバーのプロセスに `SIGTERM` を送り、サーバーのプロセスが 10 秒以内に終了しなければ `SIGKILL` を送る。
- サーバーが終了した後、mism は、作業用のディレクトリを中身ごと削除し、サーバーの作成が中断されたことを標準出力に出力して、終了ステータス 130 (`SIGINT`)または 143 (`SIGTERM`)で終了する。
- mism は、ダウンロードの間の中断を、`http_client.rs` がフラグを確認することで扱う。
- mism は、手順 8 の名前の変更の直前にもフラグを確認し、フラグが立っていれば、名前を変えずに作業用のディレクトリを削除して中断として終了する。mism が名前の変更の直前にもフラグを確認するのは、サーバーの終了から名前の変更までの間に mism が受けたシグナルで、完成したディレクトリが <パス> に残らないようにするためである。

### 性質

- d-new-1. `mism new` のどの時点で失敗や中断が起きても、<パス> にディレクトリが存在するのは、準備がすべて終わった後だけである。
- d-new-2. <パス> の検査の後に別のプロセスが <パス> に何かを作ったとき、名前の変更は失敗し、mism は、エラーを報告して作業用のディレクトリを削除する。
- d-new-3. 中断のとき、サーバーのプロセスは、mism が終了する前に終了している。
- d-new-4. mism が `SIGKILL` で止められたとき、<パス> には何も存在しないが、親ディレクトリに `.mism-tmp-` で始まるディレクトリが残ることがある。
- d-new-5. mism は <パス> の検査 (手順 1 から 3)を通信より先に行うので、通信の失敗で作業用のディレクトリが残ることはない。

### 判断

- mism が作業用のディレクトリを <パス> の隣に作るのは、名前の変更が同じファイルシステムの中でしか成り立たないからである。mism が作業用のディレクトリを `/tmp` や `~/.local/share/mism` に作ると、<パス> が別のファイルシステムにある場合に複製が必要になり、複製の途中の状態が <パス> に現れる。
- mism が `lstat` を使うのは、`stat` がリンク先の無いシンボリックリンクを「存在しない」と報告するからである。
- mism がサーバーを別のプロセスグループで起動するのは、端末の Ctrl-C がフォアグラウンドのプロセスグループ全体に `SIGINT` を送るからである。mism がサーバーのプロセスを mism と同じグループにすると、サーバーが mism より先に `SIGINT` を受けて独自の終了処理を始め、mism が止める順序を制御できない。
- mism が `SIGTERM` の後に 10 秒待ってから `SIGKILL` を送るのは、サーバーに自身の終了処理の機会を与えつつ、無限には待たないためである。初回起動の段階ではワールドが無く終了が速いので、筆者は 10 秒を十分とみなしたが、筆者はそれを確認していない。

## 初回起動

### 責務

`server_directory/initial_launch.rs` は、作業用のディレクトリの中でサーバーを 1 度起動し、終了を待ち、`eula.txt` を書き換える。
このモジュールは、仕様書の new-3 から new-8 と err-7 を満たす。

### 方法

Paper と Folia は、起動したディレクトリに `eula.txt` が無い状態で起動すると、そのディレクトリに `server.properties` と `eula=false` を書いた `eula.txt` を生成して終了し、ワールドを生成しない。
mism は、この動作を使い、サーバーの終了ステータスには頼らず、サーバーの終了後に `eula.txt` と `server.properties` が生成されているかで成否を決める。

1. mism は、環境変数 `PATH` の要素のうち絶対パスであるものを順に見て、実行できる `java` を探す。空の要素と相対パスの要素は、mism は、見ない。`java` が無ければ、mism は、Java が見つからないことを伝えるエラーを報告する。
2. mism は、`java -version` を実行して Java のメジャーバージョンを読み取り、それがバージョンの一覧のエンドポイント (`GET /v3/projects/{paper|folia}/versions`)が返す JSON の `version.java.version.minimum` より小さければ、必要な Java のバージョンと見つかった Java のバージョンを伝えるエラーを報告する。`java -version` が終わった後、mism は、中断のフラグを確認し、フラグが立っていれば中断として扱う。
3. mism は、始まりの行 (`mism: output of <jar ファイルの名前> follows`)と空行 1 行を標準出力に書き (out-2)、作業用のディレクトリをカレントディレクトリとして `java -jar <jar ファイルの名前> --nogui` を起動する。mism は、サーバーの標準入力を `/dev/null` につなぎ、サーバーの標準出力と標準エラー出力を両方とも mism の標準出力につなぐ (new-5)。
4. mism は、サーバーの終了を待つ。mism は、待つ時間に上限を設けない。サーバーが終了したら、mism は、空行 1 行と終わりの行 (`mism: output of <jar ファイルの名前> ended`)を標準出力に書く (out-2)。
5. サーバーの終了後、作業用のディレクトリに `eula.txt` と `server.properties` の両方があれば、mism は初回起動を成功とみなす。どちらかが無ければ、mism は、サーバーの作成を中止してエラーを報告する (err-7)。
6. mism は、`eula.txt` の中の `eula=false` という記述をすべて `eula=true` に書き換え、他の内容はそのまま残す (new-6)。`eula.txt` に `eula=false` という記述が 1 つも無ければ、mism は、サーバーの作成を中止してエラーを報告する。
7. mism は、`mism.toml` を作業用のディレクトリの直下に書く (new-9)。

### 性質

- d-launch-1. `java` が `PATH` に無いとき、mism は、サーバーを起動せずにエラーを報告する。
- d-launch-2. 見つかった Java のメジャーバージョンが必要な最小のバージョンより小さいとき、mism は、サーバーを起動せずにエラーを報告する。
- d-launch-3. サーバーが `eula.txt` と `server.properties` を生成せずに終了したとき、mism は、エラーを報告する。
- d-launch-4. `eula.txt` の書き換えの前後で、`eula=false` という記述以外の内容は変わらない。

### 判断

- mism が初回起動の成否の判定にサーバーの終了ステータスを使わないのは、EULA に未同意のときにサーバーがどの終了ステータスで終わるかを、mism が仮定しないためである。
- mism が `PATH` の相対パスの要素を見ないのは、mism が相対パスの要素を見ると `java` の探索の結果が mism のカレントディレクトリに依存し、サーバーは mism のカレントディレクトリとは別の作業用のディレクトリで起動されるからである。
- mism が `java -version` の後に中断のフラグを確認するのは、`java -version` のプロセスは mism と同じプロセスグループにあり、端末の Ctrl-C がそれを止めるので、mism が中断のフラグを確認しないと、中断が Java のバージョンの読み取りの失敗として報告されるからである。
- mism が `eula=false` の無い `eula.txt` をエラーにするのは、mism が初回起動をそのまま成功とみなすと、`eula=true` を含まない `eula.txt` が残り、仕様書の new-6 (`eula.txt` が `eula=true` を含むこと)に反するからである。
- mism がサーバーの標準入力を `/dev/null` につなぐのは、サーバーが標準入力からのコンソールの入力を待って止まらないようにするためである。
- 筆者は、`version.java.version.minimum` の意味を、フィールドの名前と観測した値 (Minecraft 26.x の値が 25、1.21.x の値が 21 である。筆者が 2026 年 10 月 20 日に確認した)から判断しており、PaperMC のドキュメントでは確認していない。
- mism がサーバーの終了を待つ時間に上限を設けないのは、初回起動が Mojang のサーバー jar の取得を含み、初回起動にかかる時間が回線によって大きく変わるからである。利用者は、Ctrl-C で mism を中断できる。

## mism.toml

### 責務

`server_directory/mism_toml.rs` は、`mism.toml` の型と読み書きを担う。
このモジュールは、仕様書の new-9、plg-4、dp-7 を満たす。

### 方法

```toml
software = "paper"
minecraft_version = "26.3"
software_version = 206

[[plugins]]
name = "LuckPerms"
source = "modrinth:luckperms:v5.5.71-bukkit"
file_name = "LuckPerms-Bukkit-5.5.71.jar"
sha256 = "<jar ファイルの SHA-256 の 64 桁の小文字の 16 進数>"

[[datapacks]]
name = "True Ending - Ender Dragon Overhaul"
source = "modrinth:true-ending:1.1.4d"
file_name = "tru.e-ending-1.1.4d.zip"
sha256 = "<データパックのファイルの SHA-256 の 64 桁の小文字の 16 進数>"
```

- `software` は、`"paper"` か `"folia"` の文字列である。
- `minecraft_version` は、Minecraft のバージョンの文字列である。
- `software_version` は、その Minecraft のバージョンに対する Paper (または Folia)のバージョンの整数である。
- `plugins` は、プラグインの記録の配列である。各記録は、`name` (プラグインの名前、plg-5)、`source` (取得元の指定、plg-3 の形)、`file_name` (`plugins/` の直下の jar ファイルの名前)、`sha256` (その jar ファイルの SHA-256 の小文字の 16 進数)の 4 つのキーを持つ (plg-4)。mism は、記録を、配置した順に配列の末尾に足す。
- `datapacks` は、データパックの記録の配列である。各記録は、`name` (データパックの名前、dp-8)、`source` (取得元の指定、dp-4 の形)、`file_name` (ワールドのディレクトリの `datapacks/` の直下のファイルの名前)、`sha256` (そのファイルの SHA-256 の小文字の 16 進数)の 4 つのキーを持つ (dp-7)。mism は、プラグインの記録とデータパックの記録に同じ型を使う。mism は、記録を、配置した順に配列の末尾に足す。

mism は、この 5 つのフィールドを持つ構造体を定義し、serde と toml で読み書きする。
`plugins` を持たない `mism.toml` は、プラグインの記録を 1 つも持たず、`datapacks` を持たない `mism.toml` は、データパックの記録を 1 つも持たない。今の `mism new` が作る `mism.toml` は、`plugins` も `datapacks` も持たない。
mism は、プラグインの記録が 1 つも無い `mism.toml` を書くとき `plugins` を書かず、データパックの記録が 1 つも無い `mism.toml` を書くとき `datapacks` を書かない。
mism は、`mism.toml` を読むときに、フィールドが欠けている場合 (`plugins` と `datapacks` を除く)、知らないフィールドがある場合 (記録の中を含む)、型が合わない場合に、ファイルのパスと原因を含むエラーを報告する。
mism は、`mism.toml` を、`mism new` の作業用のディレクトリに書くときも含めて、`atomic_fs.rs` の置き換えで書く。

### 性質

- d-toml-1. `plugins` を持たない `mism.toml` を mism が読んだとき、プラグインの記録は 0 個である。
- d-toml-2. プラグインの記録とデータパックの記録がどちらも 0 個の `mism.toml` を mism が書いたとき、そのファイルが持つキーは `software`、`minecraft_version`、`software_version` の 3 つだけである。
- d-toml-3. `datapacks` を持たない `mism.toml` を mism が読んだとき、データパックの記録は 0 個である。

### 判断

- 最初の 3 つのキーは、仕様書の new-9 が記録すると定めた 3 つの事実と 1 対 1 で対応する。プラグインの記録の 4 つのキーは、仕様書の plg-4 が記録すると定めた 4 つの値と 1 対 1 で対応し、データパックの記録の 4 つのキーは、仕様書の dp-7 が記録すると定めた 4 つの値と 1 対 1 で対応する。
- mism が記録にバージョンの番号を持たせないのは、Modrinth の取得元の指定 (plg-12 の形)がバージョンの番号を含み、mism がそこから取り出せるからである (「取得元の指定」の節)。`mism.toml` が同じ値を 2 か所に持つと、2 つが食い違ったときに、mism はどちらが正しいかを決められない。
- データパックの記録は、ファイルを置いたワールドのディレクトリを持たない。mism は、データパックの記録のファイルを、そのときの `level-name` の値から決めたワールドのディレクトリの `datapacks/` の中で探す (「プラグインとデータパックを置くディレクトリ」の節)。利用者が `level-name` を変えた後は、`mism datapack list` は、前のワールドのディレクトリに置いた記録のファイルを `missing` と出力する。
- mism が記録の無いときに `plugins` と `datapacks` を書かないのは、プラグインとデータパックを使わないサーバーの `mism.toml` を、今の `mism new` が作るものと同じ 3 つのキーのままにするためである。
- mism が `mism.toml` を置き換えで書くのは、書き込みの途中で mism が止まっても、`mism.toml` が古い内容か新しい内容のどちらかであるようにするためである (plg-8)。`mism new` でも同じ方法で書くのは、書き方を 1 つにするためである。
- 筆者が `software` と `software_version` を対にするのは、将来 `software = "fabric"` を入れてもキーの名前を変えずに済むようにするためである。
- mism が知らないフィールドをエラーにするのは、古い mism が新しい `mism.toml` を誤って読むことを防ぐためである。

## mism versions の手順

### 責務

`subcommands/versions.rs` は、`mism versions` の手順の全体を担う。
このモジュールは、仕様書の ver-1 から ver-3 を満たす。

### 方法

1. mism は、バージョンの一覧のエンドポイント (`GET /v3/projects/{paper|folia}/versions`)を 1 回呼び、Minecraft のバージョンの一覧を得る。
2. mism は、一覧を `version.id` のリリースが新しい順に並べる (ver-1)。
3. mism は、並べた一覧の各要素について、`version.id`、タブ 1 文字、`version.support.status` の順につないだ文字列を、`output.rs` を通して (先頭に `mism: ` を付けて) 1 行ずつ標準出力に書く (ver-1、ver-2、out-1)。

呼び出しが失敗した場合、mism は、手順 3 に進まないので標準出力に何も出力せず、バージョンの列挙を中止してエラーを報告する (ver-3)。

### 性質

- d-versions-1. `mism versions` は、API を 1 回だけ呼ぶ。

## mism plugin と mism datapack の対象のサーバーと共通の手順

### 責務

`subcommands/plugin/mod.rs` は `mism plugin` の 8 つのサブコマンドを、`subcommands/datapack/mod.rs` は `mism datapack` の 8 つのサブコマンドを振り分け、それぞれのサブコマンドが共有する値 (この節の表)と、変更が反映される旨の出力を持つ。`subcommands/target_server.rs` は、2 組のサブコマンドが使う、対象のサーバーの決定を持ち、`server_directory/mod.rs` は、カレントディレクトリの `mism.toml` を読む。`interruption.rs` は、変え始める直前の中断のフラグの確認を持ち、`subcommands/terminal_asker.rs` は、2 組のサブコマンドが Modrinth のファイルの選択に渡す、標準入力で値を尋ねる処理を持つ。
これらのモジュールは、仕様書の plg-1、plg-2、plg-7、dp-1、dp-2、dp-10 を満たし、各サブコマンドが plg-8 と dp-11 を満たすための処理の順序を定める。

### 方法

mism は、対象のサーバーを次の手順で決める。

1. mism は、カレントディレクトリの直下の `mism.toml` を読む。読み込みが「ファイルが存在しない」という理由で失敗したとき、mism は、対象のサーバーが無いとする (plg-1、plg-2、dp-1、dp-2)。読み込みがそれ以外の理由で失敗したとき、または `mism.toml` の形が正しくないとき (「mism.toml」の節)、mism は、処理を中止してエラーを報告する。
2. `place`、`install`、`remove`、`--global` の無い `list` は、対象のサーバーが無ければ、通信もファイルの変更もせずに処理を中止してエラーを報告する (plg-1、dp-1)。
3. `versions` と `inspect` は、対象のサーバーが無ければ、対象のサーバー無しで処理を続ける (plg-2、dp-2)。
4. `search`、`fetch`、`list --global` は、`mism.toml` を読まない。

`mism plugin` と `mism datapack` のサブコマンドは、どちらも `subcommands/target_server.rs` のこの手順で対象のサーバーを決める。
対象のサーバーのディレクトリは、カレントディレクトリの絶対パスである。対象のサーバーのサーバーソフトウェアは `mism.toml` の `software` の値 (`paper` か `folia`)であり、対象のサーバーの Minecraft のバージョンは `mism.toml` の `minecraft_version` の値である。

置くディレクトリ (プラグインでは `plugins/`、データパックではワールドのディレクトリの `datapacks/`)を変えたサブコマンド (`place`、`install`、記録のファイルを置くディレクトリから削除した `remove`)は、結果の記述の後に、`mism: the change takes effect when the server starts next` という文字列を書く (plg-7、dp-10)。mism は、サーバーが動いているかを確かめない (plg-7、dp-10)。

各サブコマンドは、plg-8 と dp-11 を満たすために、次の順で処理を進める。

1. サブコマンドは、通信、利用者への問い、検証、置くディレクトリと `mism.toml` を変えずにできる失敗の確認 (e.g. place-3、place-4、dplace-3、dplace-4)を、すべて先に行う (plg-8、dp-11)。
2. サブコマンドは、置くディレクトリ (データパックでは、ワールドのディレクトリのパスの中のまだ無いディレクトリを含む)か `mism.toml` を変え始める直前に、中断のフラグを確認する。
3. サブコマンドは、途中で失敗したときに元に戻せる順に置くディレクトリと `mism.toml` を変え (「プラグインとデータパックを置くディレクトリ」の節)、途中で失敗したら、それまでの変更を戻してからエラーを報告する (plg-8、dp-11)。

`subcommands/terminal_asker.rs` は、`modrinth_api/selection.rs` が利用者に値を尋ねるための処理を、`prompt.rs` を使って実装する。その処理は、標準入力が端末であり利用者が `--yes` を付けていないときだけ尋ね、それ以外のときは尋ねられないという結果を返す (plg-11、dp-5)。

プラグインとデータパックで違う値は、次のとおりである。`subcommands/plugin/mod.rs` がプラグインの値を、`subcommands/datapack/mod.rs` がデータパックの値を持ち、両方が使うモジュール (「複数のモジュールに制約を課す決定」の節)に渡す。Modrinth の検索とファイルの選択の条件は、「Modrinth の API」の節の表にある。

| 値 | プラグイン | データパック |
|---|---|---|
| エラーの案内に書くサブコマンド | `mism plugin` のサブコマンド | `mism datapack` のサブコマンド |
| ファイルの名前の末尾 (URL と手元のファイルの名前の決定と検査) | `.jar` | `.zip` |
| 置くディレクトリ | 対象のサーバーのディレクトリの `plugins/` (`server_directory/plugins.rs` が決める) | 対象のサーバーのワールドのディレクトリの `datapacks/` (dp-3。`server_directory/datapacks.rs` が決める) |
| キャッシュのディレクトリの中のディレクトリ | `plugins/` | `datapacks/` |
| `mism.toml` の記録の配列 | `plugins` | `datapacks` |
| 一覧の `unmanaged` のファイル (list-2、dlist-2) | 名前が `.jar` で終わるファイル | 名前が `.zip` で終わるファイル |

### 性質

- d-pcmd-1. `mism plugin` のサブコマンドが `SIGINT` または `SIGTERM` による中断で終了したとき、対象のサーバーの `plugins/` と `mism.toml` は、そのサブコマンドの実行の前と同じである。
- d-pcmd-2. `mism datapack` のサブコマンドが `SIGINT` または `SIGTERM` による中断で終了したとき、対象のサーバーのワールドのディレクトリと `mism.toml` は、そのサブコマンドの実行の前と同じである。

### 判断

- 各サブコマンドが変え始めた後に中断のフラグを確認しないのは、サブコマンドが置くディレクトリと `mism.toml` を変える途中でやめると、2 つが食い違うからである。変える処理は、キャッシュのディレクトリのファイルの複製、ディレクトリの作成、名前の付与と変更、`mism.toml` の置き換えだけなので、短い時間で終わる。
- 筆者が `mism plugin` と `mism datapack` の 8 つずつのサブコマンドを別々のモジュール (`subcommands/plugin/` と `subcommands/datapack/`)で実装し、両方に共通の処理だけを両方が使うモジュールにするのは、利用者にとってプラグインとデータパックが別の機能だからである。筆者は、2 組のサブコマンドを、プラグインかデータパックかを表す値を受け取る 1 組のモジュールで実装する方法を退けた。その方法では、`mism plugin` の使い方の説明とエラーの文を、データパックにも当てはまる曖昧な文 (e.g. jar ファイルではなく「ファイル」)にすることになり、`docs/development-rules.md` の「コードの規則」の 13 番目 (全体のコード量が減る変更を選ぶ)を理由に、利用者に見える文を損なうことになるからである。
- mism は、同じサーバーのディレクトリで同時に動く 2 つの mism を排他しない。2 つの mism が同時に `mism.toml` を置き換えると、先に置き換えた方の記録が失われることがある。仕様書は、同じサーバーのディレクトリで同時に動く 2 つの mism の振る舞いを定めていない。

## 取得元の指定

### 責務

`fetch_source.rs` は、<取得元の指定> を 3 つの形に分け、Modrinth の取得元の指定の解析を `modrinth_source.rs` に任せ、URL からプラグイン (またはデータパック)の名前を作り、手元のファイルの名前を確かめる。`modrinth_source.rs` は、`modrinth:` の取得元の指定を解析し、識別子の文字の種類を確かめ、`modrinth:` の取得元の指定の文字列を組み立て、取得元の指定の文字列から識別子とバージョンの番号を取り出す。
この 2 つのモジュールは、仕様書の plg-3、dp-4、vers-4、dvers-4、fetch-2 と dfetch-2 の名前の作り方、plg-5 と dp-8 の手元のファイルの名前の決め方、plg-12 と dp-6 の文字列の形を満たす。

### 方法

mism は、<取得元の指定> を、先頭の文字列によって次の 3 つに分ける (plg-3、dp-4)。この分け方は、プラグインとデータパックで同じである。`fetch_source.rs` は、手元のファイルの名前を確かめるときと URL から名前を作るときに使うファイルの名前の末尾 (`.jar` か `.zip`)を、`subcommands/plugin/` と `subcommands/datapack/` から受け取る。

1. `modrinth:` で始まる文字列は、Modrinth の取得元の指定である。`modrinth_source.rs` は、`modrinth:` の後を `:` で区切り、最初の要素を識別子とし、残りの要素 (0 個以上。空の要素を含む)を、ファイルを絞る属性 (plg-10、dp-5)の値として、与えられた順に並べる。
2. `https://` または `http://` で始まる文字列は、ファイルの URL である。mism は、<取得元の指定> の先頭とこの 2 つの文字列を、大文字と小文字を区別して比べる。
3. それ以外の文字列は、手元のファイルのパスである。相対パスは、カレントディレクトリを基準にする。

mism は、識別子が 3 文字以上 64 文字以下であり、すべての文字が ASCII の英字、ASCII の数字、`_`、`!`、`@`、`$`、`(`、`)`、`` ` ``、`.`、`+`、`,`、`"`、`-`、`'` のどれかであることを確かめる。そうでなければ、mism は、通信せずに処理を中止してエラーを報告する (plg-3、dp-4)。
mism は、手元のファイルのパスについて、そのパスがシンボリックリンクをたどった先で通常のファイルを指し、パスの最後の要素が UTF-8 の文字列であり、ファイルの名前の末尾 (プラグインでは `.jar`、データパックでは `.zip`。「mism plugin と mism datapack の対象のサーバーと共通の手順」の節)で終わることを確かめる。そうでなければ、mism は、処理を中止してエラーを報告する (plg-3、dp-4)。
mism は、URL の形をこのモジュールでは確かめない。ureq が URL として解釈できない文字列に対して、mism は、ダウンロードの失敗としてエラーを報告する。

`mism plugin versions` と `mism datapack versions` の引数は、`modrinth:` に識別子だけを続けた文字列でなければならない。`modrinth:` で始まらない文字列、識別子の後に `:` がある文字列、識別子が上の文字の種類に合わない文字列に対して、mism は、通信せずに処理を中止してエラーを報告する (vers-4、dvers-4)。

URL から名前を作るとき、mism は、URL の `://` の後から最初の `?` または `#` の前までを取り、その中の最後の `/` より後の部分を、URL のパスの最後の要素とする。その中に `/` が無いとき (e.g. `https://example.com`)、最後の要素は空である。mism は、最後の要素がファイルの名前の末尾 (プラグインでは `.jar`、データパックでは `.zip`)で終わっていればその 4 文字を除き、残りをプラグイン (またはデータパック)の名前とする (fetch-2、dfetch-2)。mism は、最後の要素のパーセントエンコーディングを戻さない (e.g. `https://example.com/dl/My%20Plugin.jar?token=1` から作るプラグインの名前は `My%20Plugin` である)。
手元のファイルのプラグイン (またはデータパック)の名前は、パスの最後の要素から末尾の `.jar` (または `.zip`)を除いたものであり、そのファイルの名前は、パスの最後の要素である (plg-5、dp-8)。

mism は、`modrinth:` の取得元の指定の文字列を、`modrinth:` と識別子の後に、属性の値を 1 つずつ `:` に続けて組み立てる (plg-12、dp-6)。
mism は、取得元の指定の文字列から、識別子とバージョンの番号を次のとおり取り出す。`modrinth:` の取得元の指定では、識別子は `modrinth:` の後の最初の要素であり、バージョンの番号は 2 番目の要素である。2 番目の要素が無ければ、バージョンの番号は無い。URL と手元のファイルの取得元の指定は、識別子もバージョンの番号も持たない。mism が `source.toml` と `mism.toml` の記録について出力するバージョンの番号 (fetch-5、insp-2、place-5、list-2、list-4、dfetch-5、dinsp-2、dplace-5、dlist-2、dlist-4)は、すべてこの方法で取り出したものである。mism が `mism.toml` の記録の取得元の指定と比べる識別子 (insp-3、dinsp-3)も、この方法で取り出したものである。

### 性質

- d-source-1. 利用者が、名前が `.jar` で終わらない手元のファイルのパスを `mism plugin` のサブコマンドに <取得元の指定> として与えたとき、mism は、エラーを報告し、キャッシュのディレクトリに何も保存しない。
- d-source-2. 利用者が `https://example.com/dl/My%20Plugin.jar?token=1#top` から取得するとき、mism が作るプラグインの名前は `My%20Plugin` である。
- d-source-3. 利用者が `modrinth:../search` を与えたとき、mism は、Modrinth にリクエストを送らずにエラーを報告する。
- d-source-4. 利用者が、名前が `.zip` で終わらない手元のファイルのパス (e.g. `pack.jar`)を `mism datapack` のサブコマンドに <取得元の指定> として与えたとき、mism は、エラーを報告し、キャッシュのディレクトリに何も保存しない。

### 判断

- 筆者は、Modrinth の slug (識別子)の文字の種類 (`^[\w!@$()`.+,"\-']{3,64}$`)を、2026 年 10 月 20 日と 21 日に Modrinth の API のドキュメントで確認した。筆者は、Modrinth の API の実装 (labrinth)の 2026 年 10 月 21 日の版が、新しく作る slug を `^[a-zA-Z0-9._-]+$` に限り、バージョンの番号を `^[a-zA-Z0-9!@$()`.+,_"-]+$` に限っていることも、同じ日にソースコードで確認した。mism がドキュメントの文字の種類で識別子を確かめるのは、実装が今の規則にする前に作られた slug も受け付けるためである。mism は、ドキュメントの `\w` を、ASCII の英字、ASCII の数字、`_` とみなす。筆者は、ASCII 以外の文字を含む slug があるかを確認していない。
- mism が識別子を通信の前に確かめるのは、mism が識別子を URL のパスに入れるので、`/` や `..` を含む識別子が別のエンドポイントを指すからである。
- mism が手元のファイルの名前が `.jar` (または `.zip`)で終わることを求めるのは、plg-5 と dp-8 が名前をファイルの名前から `.jar` (または `.zip`)を除いて作ると定め、mism がそのファイルを同じ名前で置くディレクトリに置くからである。データパックの名前の末尾を `.zip` に限るのは、2026 年 10 月 21 日に筆者が調べた Modrinth のデータパックのファイル 659 個の名前が、全部 `.zip` で終わっていたからでもある (「Modrinth の API」の節)。
- mism が URL のパスの最後の要素のパーセントエンコーディングを戻さないのは、mism がそれを戻すと、変更を行う者が不正な並び (e.g. `%ZZ`)と UTF-8 でないバイト列の扱いを決めることになり、利用者は fetch-2 と dfetch-2 の問いで別の名前を入力できるからである。
- `modrinth:` の取得元の指定は、属性の値を `:` で区切るので、`:` を含む値を表せない。バージョンの番号は、上の実装の規則により `:` を含まない。`:` を含む値の扱いは、「Modrinth のファイルの選択」の節が定める。
- mism が `source.toml` と `mism.toml` の記録にバージョンの番号を書かずに取得元の指定から取り出すのは、同じ値を 2 か所に持たないためである。
- 筆者が `modrinth:` の取得元の指定の処理を `fetch_source.rs` から `modrinth_source.rs` に分けるのは、`plugin_compatibility.rs`、`datapack_compatibility.rs`、`modrinth_api/selection.rs` が、URL と手元のファイルの処理を使わずに、識別子の取り出しと取得元の指定の組み立てだけを使うからである。

## ファイルの名前の検査

### 責務

`file_name.rs` は、mism が外部から得たプラグインとデータパックの名前と、jar ファイルとデータパックのファイルの名前を、mism がファイルやディレクトリの名前に使う前に検査する。
このモジュールは、仕様書の plg-6 と dp-9 を満たす。

### 方法

名前の検査では、mism は、プラグイン (またはデータパック)の名前が空、`.`、`..` のどれかであるとき (plg-6、dp-9)と、名前が NUL 文字を含むときに、処理を中止してエラーを報告する。
ファイルの名前の検査では、mism は、ファイルの名前が空、`.`、`..` のどれかであるときと、名前が `/` か NUL 文字を含むときに、処理を中止してエラーを報告する。

mism は、次の値を検査する。

- mism は、取得するすべてのプラグインとデータパックの名前 (Modrinth の `title`、URL から作った名前、利用者が fetch-2 と dfetch-2 の問いに入力した名前、手元のファイルから作った名前)を、名前の検査に通す。
- mism は、Modrinth のファイルの `filename`、URL から取得するときのファイルの名前 (名前に `.jar` か `.zip` を付けたもの)、手元のファイルの名前、`source.toml` の `file_name`、`mism.toml` の記録の `file_name` を、ファイルの名前の検査に通す。

`/` を含む名前は、名前の検査を通る。mism は、名前をキャッシュのディレクトリの中のディレクトリの名前に使うとき、`/` を `-` に置き換える (「キャッシュのディレクトリのプラグインとデータパック」の節)。URL から取得するときの名前が `/` を含むとき、その名前に `.jar` (または `.zip`)を付けたファイルの名前はファイルの名前の検査を通らないので、mism は、エラーを報告する。

### 性質

- d-name-1. `mism.toml` の記録の `file_name` が `../server.properties` のとき、`mism plugin remove` は、エラーを報告し、何も削除しない。
- d-name-2. 利用者が fetch-2 の問いに `a/b` と答えたとき、mism は、エラーを報告し、キャッシュのディレクトリに何も保存しない。

### 判断

- mism が名前を検査するのは、外部から得た名前が `/` や `..` を含むと、mism がキャッシュのディレクトリや置くディレクトリの外のファイルを書いたり消したりすることになるからである。
- Linux のファイルの名前に使えないバイトは `/` と NUL だけであり、`.` と `..` はディレクトリそのものと親のディレクトリを指すので、mism は、空の名前、`.`、`..`、`/` か NUL を含む名前だけを拒む。

## Modrinth の API

### 責務

`modrinth_api/` は、Modrinth の API を呼ぶ。
このモジュールは、仕様書の srch-1 と dsrch-1 の検索、vers-1、dvers-1、plg-10、dp-5 のバージョンの一覧、plg-3 と dp-4 の識別子の確認、plg-5 と dp-8 の Modrinth での名前、vers-2、insp-2、dvers-2、dinsp-2 の必須の依存先の識別子、fetch-1 と dfetch-1 のファイルの URL と SHA-512 を担い、srch-4、vers-5、dsrch-4、dvers-5 の通信の失敗を報告する。

### 方法

mism は、Modrinth の API のバージョン 2 (ベース URL `https://api.modrinth.com/v2`)を使う。
環境変数 `MISM_MODRINTH_URL` が設定されているとき、mism は、その値をベース URL として使う。

mism は、プラグインとデータパックで、次の 4 つを変える。`subcommands/plugin/` がプラグインの値を、`subcommands/datapack/` がデータパックの値を持つ。2 つのモジュールは、検索の `facets` の値と検索の記述に使う Modrinth のバージョンの条件を `subcommands/modrinth_search.rs` に、バージョンの列挙に使う Modrinth のバージョンの条件を `subcommands/modrinth_versions.rs` に、ファイルの選択の候補の条件を `subcommands/modrinth_fetch.rs` を通して `modrinth_api/selection.rs` に渡す。

| 値 | プラグイン | データパック |
|---|---|---|
| 検索の `facets` の値 (srch-1、dsrch-1) | `[["project_type:plugin"]]` | `[["project_type:datapack"]]` |
| 検索の記述に使う Modrinth のバージョン (srch-2、dsrch-2) | `loaders` が `paper` か `folia` を含むもの | `loaders` が `datapack` を含むもの |
| バージョンの列挙に使う Modrinth のバージョン (vers-1、dvers-1) | すべての Modrinth のバージョン | `loaders` が `datapack` を含むもの |
| ファイルの選択の候補 (plg-10、dp-5) | すべての Modrinth のバージョンのすべてのファイル | `loaders` が `datapack` を含む Modrinth のバージョンのファイルのうち、`file_type` が `null` であるもの |

mism は、次の 5 つのエンドポイントだけを使う。

検索のエンドポイントは、`GET /v2/search` である。mism は、クエリ文字列の `query` に <語> を、`facets` に呼び出す側のモジュールから受け取った値を、`limit` に件数を入れる。mism は、レスポンスの `hits` の各要素から、`project_id`、`slug` (識別子)、`title` (プラグインかデータパックの名前)を、`hits` の順のまま読む。

プロジェクトのエンドポイントは、`GET /v2/project/{識別子}` である。mism は、識別子の中の `"` を `%22` に、`` ` `` を `%60` に置き換えてから、識別子をパスに入れる。mism は、レスポンスの `id`、`slug`、`title` を読む。

バージョンの一覧のエンドポイントは、`GET /v2/project/{プロジェクトの id}/version` である。mism は、クエリ文字列の `include_changelog` に `false` を入れる。mism は、レスポンスの各要素 (Modrinth のバージョン)から、`id`、`version_number` (バージョンの番号)、`version_type` (リリースの種別)、`loaders` (対応するソフトウェア。プラグインでは対応するサーバーソフトウェア)、`game_versions` (対応する Minecraft のバージョン)、`date_published` (公開日時)と、`dependencies` の各要素の `project_id`、`version_id`、`dependency_type` と、`files` の各要素の `filename`、`url`、`hashes.sha512`、`file_type` を読む。`file_type` が無いファイルについて、mism は、`file_type` を `null` とみなす。mism は、`date_published` を RFC 3339 の時刻として読み、読めなければ、レスポンスが期待した JSON でないことを伝えるエラーを報告する。

複数のプロジェクトのエンドポイントは、`GET /v2/projects` である。mism は、クエリ文字列の `ids` に、プロジェクトの id を重複を除いて昇順に並べた JSON の配列 (e.g. `["3wmN97b8"]`)を入れる。mism は、レスポンスの各要素の `id` と `slug` を読む。

複数のバージョンのエンドポイントは、`GET /v2/versions` である。mism は、クエリ文字列の `ids` に、Modrinth のバージョンの id を重複を除いて昇順に並べた JSON の配列を入れる。mism は、レスポンスの各要素の `id` と `project_id` を読む。

`modrinth_api/mod.rs` は、この 5 つの操作をトレイトとして定義し、HTTP で呼ぶ実装は `http_client.rs` を使う。
mism は、Modrinth にリクエストを送る前に毎回中断のフラグを確認し、フラグが立っていれば、リクエストを送らずに中断のエラーを返す。
プロジェクトのエンドポイントが HTTP ステータス 404 を返したとき、mism は、その識別子のプロジェクトが Modrinth に無いことを伝えるエラーを報告する。
プロジェクトのエンドポイントが返した `slug` が利用者の与えた識別子と一致しない (大文字と小文字の違いを含む)とき、mism は、与えた文字列は識別子ではないとして、処理を中止してエラーを報告する (plg-3、dp-4)。このエラーは、Modrinth が返した `slug` を含む。
どのエンドポイントが HTTP ステータス 429 を返したときも、mism は、Modrinth がリクエストの数の上限を理由にリクエストを断ったことを伝えるエラーを報告する。
mism は、それ以外の通信の失敗を、`http_client.rs` のエラーのまま報告する (srch-4、vers-5、dsrch-4、dvers-5)。

`modrinth_api/mod.rs` は、Modrinth のバージョンの必須の依存先の識別子を、次の手順で決める。必須の依存先は、`dependencies` の要素のうち、`dependency_type` が `required` であるものである。

1. mism は、`project_id` を持つ必須の依存先から、その `project_id` を集める。
2. mism は、`project_id` を持たず `version_id` を持つ必須の依存先の `version_id` を集め、複数のバージョンのエンドポイントを 1 回呼んで、それぞれの Modrinth のバージョンの `project_id` を得る。そのような依存先が無ければ、mism は、このエンドポイントを呼ばない。
3. mism は、`project_id` も `version_id` も持たない必須の依存先を、識別子の一覧に入れない。
4. mism は、手順 1 と手順 2 で集めたプロジェクトの id について、複数のプロジェクトのエンドポイントを 1 回呼び、各 id の `slug` を、その依存先の識別子とする。集めた id が無ければ、mism は、このエンドポイントを呼ばない。集めた id のどれかがレスポンスに無ければ、mism は、その id を含むエラーを報告する。

mism は、`versions` では列挙するすべての Modrinth のバージョンの必須の依存先を、取得では選んだファイルが属する 1 つの Modrinth のバージョンの必須の依存先を、この手順でまとめて決める。

### 性質

- d-modrinth-1. mism が Modrinth にリクエストを送っている途中で `SIGINT` を受けたとき、mism は、それより後に新しいリクエストを送らない。
- d-modrinth-2. 必須の依存先が `project_id` を持たず `version_id` だけを持つとき、mism は、その `version_id` の Modrinth のバージョンが属するプロジェクトの `slug` を、その依存先の識別子とする。
- d-modrinth-3. 必須の依存先が `project_id` も `version_id` も持たないとき、mism は、その依存先を識別子の一覧に入れない。
- d-modrinth-4. Modrinth が HTTP ステータス 429 を返したとき、mism は、1 分待ってからもう一度実行するよう利用者に案内するエラーを報告する。

### 判断

- Modrinth のプラグインの識別子 (plg-3)は、Modrinth のプロジェクトの `slug` である。mism が識別子を slug に限りプロジェクトの id を受け付けないのは、利用者が識別子を知る手段 (`mism plugin search` の出力と Modrinth のページの URL)がどちらも slug であり、id を受け付けると `mism.toml` と出力の取得元の指定が利用者の与え方によって変わるからである。
- 筆者は、プロジェクトのエンドポイントが slug の大文字と小文字を区別せずにプロジェクトを返し (e.g. `LuckPerms` に対して slug が `luckperms` のプロジェクトを返す)、プロジェクトの id (e.g. `Vebnzrzj`)に対してもそのプロジェクトを返すことを、2026 年 10 月 21 日に確認した。そのため、mism は、返った `slug` と与えた識別子を比べる (plg-3)。
- 筆者は、検索の `facets` に `project_type:plugin` を入れると、`project_type` が `mod` であり `all_project_types` が `plugin` を含むプロジェクト (e.g. LuckPerms)も返ることを、2026 年 10 月 21 日に確認した。筆者は、`facets` に `project_type:datapack` を入れると `project_type` が `mod` であるプロジェクト (e.g. Terralith、True Ending)が返ることも、同じ日に確認した。
- 筆者は、Terralith と True Ending のように、同じプロジェクトの中で、`loaders` が `datapack` の Modrinth のバージョン (ファイルは zip 形式)と、`loaders` が `fabric` などのモッドの Modrinth のバージョン (ファイルは jar 形式)を公開しているプロジェクトがあることを、2026 年 10 月 21 日に確認した。e.g. Terralith の 83 個の Modrinth のバージョンのうち、`loaders` が `datapack` を含むものは 19 個であり、True Ending の `1.1.4` の番号を持つ Modrinth のバージョンは、`loaders` が `datapack` のものと `fabric,forge,quilt` のものの 2 つである。そのため、データパックの検索の記述、バージョンの列挙、ファイルの選択の候補は、`loaders` が `datapack` を含む Modrinth のバージョンに限られる (dsrch-2、dvers-1、dp-5)。
- 筆者は、Modrinth の API のドキュメントが、ファイルの `file_type` を `required-resource-pack`、`optional-resource-pack`、`sources-jar`、`dev-jar`、`javadoc-jar`、`unknown`、`signature` のいずれかか `null` とし、主にデータパックにリソースパックを足すために使う値と説明していることを、2026 年 10 月 21 日に確認した。筆者は、同じ日に、`project_type:datapack` の検索でダウンロードの数が多い 40 個のプロジェクトについて、`loaders` が `datapack` を含む Modrinth のバージョンの 659 個のファイルを調べ、その全部の名前が `.zip` で終わり、`file_type` が `null` でないファイルが、`optional-resource-pack` の 8 個と `required-resource-pack` の 2 個であることを確認した。mism がデータパックの候補から `file_type` が `null` でないファイルを除くのは、そのファイルがデータパックではないからである (dp-5)。
- mism がバージョンの一覧のエンドポイントのパスにプロジェクトの id を入れるのは、検索の結果のプロジェクトについても、プロジェクトのエンドポイントを呼ばずにバージョンの一覧を得られるからである。
- 筆者は、Modrinth の API のドキュメントが `include_changelog=false` をほとんどの場合に使うよう勧めていることと、LuckPerms のバージョンの一覧の大きさがこのクエリ文字列で 106164 バイトから 90354 バイトに減ることを、2026 年 10 月 21 日に確認した。mism は、変更の履歴 (`changelog`)を使わない。
- mism が依存先の slug を複数のプロジェクトのエンドポイントでまとめて得るのは、Modrinth の API が 1 つの IP アドレスからのリクエストを 1 分間に 300 回までに制限しているからである。筆者は、この上限を 2026 年 10 月 20 日に Modrinth のドキュメントで確認し、2026 年 10 月 21 日にレスポンスのヘッダー (`x-ratelimit-limit: 300`)で確認した。筆者は、複数のプロジェクトのエンドポイントが、存在しない id をエラーにせずにレスポンスから除くことと、複数のバージョンのエンドポイントが各要素に `project_id` を持つことを、2026 年 10 月 21 日に確認した。
- 筆者は、Modrinth の API のドキュメントが依存先の `file_name` を「主にモッドパックの外部の依存先を示すために使う」と説明し、`project_id` と `version_id` をどちらも `null` になり得る値としていることを、2026 年 10 月 21 日に確認した。`project_id` も `version_id` も持たない依存先は Modrinth のプロジェクトを指さないので、識別子 (plg-3)を持たない。筆者は、`version_id` だけを持つ依存先を、実際のレスポンスでは見ていない。
- mism がリクエストの前に毎回中断のフラグを確認するのは、`mism plugin search` と `mism datapack search` が最大 101 回のリクエストを順に送り、`http_client.rs` が JSON の取得の間はフラグを確認しないからである。

## Modrinth のファイルの選択

### 責務

`modrinth_api/selection.rs` は、`modrinth:` の取得元の指定が指すファイルを、Modrinth のバージョンの一覧から、呼び出す側のモジュール (`subcommands/plugin/` か `subcommands/datapack/`)から受け取った条件 (「Modrinth の API」の節の表)に従って 1 つに絞り、そのファイルの plg-12 (または dp-6)の形の取得元の指定を組み立てる。
このモジュールは、仕様書の plg-10、plg-11、plg-12、inst-2、dp-5、dp-6、dinst-2 を満たす。

### 方法

候補は、バージョンの一覧のエンドポイントが返した Modrinth のバージョンの `files` の要素 (ファイル)のうち、呼び出す側のモジュールから受け取った候補の条件 (「Modrinth の API」の節の表)に合うものである。各候補は、属性として、そのファイルが属する Modrinth のバージョンの `version_number`、`loaders`、`game_versions`、`version_type` と、ファイルの `filename` を持つ (plg-10、dp-5)。
mism は、エラーと問いの中で、この 5 つの属性を、順に `version`、`software`、`minecraft_version`、`release_type`、`file_name` という名前で呼ぶ。`software` と `minecraft_version` の値は一覧である。
mism が候補を絞るのに使う属性は、プラグインではこの 5 つであり、データパックでは `software` を除いた 4 つである (dp-5)。`subcommands/plugin/` と `subcommands/datapack/` が、使う属性の一覧を `modrinth_api/selection.rs` に渡す。

mism は、候補が属性に持つ値を示すとき (問いとエラー)、`version` の値を、その番号を持つ Modrinth のバージョンの `date_published` のうち最も新しいものが新しい順に並べ、その公開日時が同じ値どうしを文字列の昇順に並べる。mism は、それ以外の属性の値を文字列の昇順に並べる。一覧の属性では、mism は、候補の一覧の要素を、重複を除いて並べる (plg-11)。

mism は、次の手順で候補を絞る。

1. mism は、候補を、候補の条件に合うすべてのファイルにする (plg-10、dp-5)。候補が 1 つも無ければ、mism は、そのプラグイン (またはデータパック)のファイルが Modrinth に無いことを伝えるエラーを報告する。
2. mism は、`version` の値を決める (plg-10)。利用者が値を与えていれば、mism は、その最初の値を `version` の値とする。利用者が値を与えておらず、`mism plugin install` か `mism datapack install` の `--lazy` で選ぶとき (inst-2、dinst-2)、mism は、手順 7 に進む。利用者が値を与えておらず、候補が 1 種類の `version` の値しか持たなければ、mism は、その値を `version` の値とする。それ以外のとき、mism は、手順 6 のとおり値を尋ねる (plg-11)。
3. mism は、候補を、その属性にその値を持つものだけにする。一覧の属性では、mism は、候補を、その属性の一覧にその値を含むものだけにする (plg-10)。絞った後の候補が 1 つも無ければ、mism は、処理を中止し、属性の名前と、絞る前の候補がその属性に持つ値を示すエラーを報告する (plg-10)。
4. 候補が 1 つになったとき、mism は、その候補を、取得元の指定が指すファイルとする (plg-10)。そのとき利用者が与えた値が残っていれば、mism は、処理を中止し、残った値と、選んだファイルの取得元の指定を示すエラーを報告する (plg-10)。
5. 候補が 2 つ以上残っていれば、mism は、プラグイン (またはデータパック)が使う属性のうち `version` より後のものを上に書いた順に見て、まだ見ていない次の属性について次のことを行う (plg-10、dp-5)。候補がその属性に持つ値 (一覧の属性では、候補が持つ一覧そのもの。候補の一覧が全部同じなら 1 種類である)が 1 種類だけなら、mism は、その属性を飛ばす。2 種類以上なら、mism は、利用者が与えた値が残っていれば次の値をその属性の値とし、残っていなければ手順 6 のとおり尋ねて値を決め、手順 3 と手順 4 を行う。`file_name` を見た後にも候補が 2 つ以上残っていれば、mism は、どの属性でも区別できないファイルが複数あることを伝え、それらのファイルの URL を示すエラーを報告する。
6. 値を尋ねるとき、mism は、呼び出す側のモジュールから受け取った尋ねる処理 (`subcommands/terminal_asker.rs`)を使う。尋ねられるとき (標準入力が端末であり、利用者が `--yes` を付けていないとき)、mism は、次の文字列を書き、標準入力から改行までの回答を 1 つ読む (plg-11)。入力が示した値のどれかと完全に一致すれば、mism は、それをその属性の値とする。一致しなければ (空の入力を含む)、mism は、処理を中止してエラーを報告する (plg-11)。尋ねられないとき、mism は、尋ねずに処理を中止し、属性の名前と候補が持つ値を示すエラーを報告する (plg-11)。

   ```
   mism: <途中までの取得元の指定> has files with these values of <属性の名前>:
   mism: <1 つ目の値>
   mism: <2 つ目の値>
   mism: <属性の名前>: 
   ```

   <途中までの取得元の指定> は、`modrinth:` と識別子に、それまでに決めた属性の値を手順 8 の形で続けたもの (e.g. `modrinth:fastasyncworldedit:2.16.1`)である。最後の文字列 (`mism: <属性の名前>: `)は改行で終わらず、利用者は、その後に回答を入力する。
7. `--lazy` で選ぶとき (inst-2、dinst-2)、mism は、候補が属する Modrinth のバージョンのうち、対象のサーバーに合うものの中で、`date_published` が最も新しいもの 1 つを選ぶ。対象のサーバーに合う Modrinth のバージョンは、プラグインでは、対象のサーバーと照らし合わせた結果 (vers-3、「プラグインの検証の値と判定」の節)が `match` であるものであり (inst-2)、データパックでは、対象のサーバーと照らし合わせた結果 (dvers-3、「データパックの検証の値と判定」の節)が `match` であるもの (`game_versions` が対象のサーバーの Minecraft のバージョンを文字列の完全な一致で含むもの)である (dinst-2)。呼び出す側のモジュールが、この条件と、手順 9 で使う対象のサーバーの値を `modrinth_api/selection.rs` に渡す。公開日時が最も新しいものが複数あれば、mism は、バージョンの一覧のエンドポイントが返した順で先のものを選ぶ。mism は、候補をその Modrinth のバージョンのファイルだけにし、その `version_number` を `version` の値として、手順 4 に進む (inst-2、dinst-2)。対象のサーバーに合う Modrinth のバージョンが 1 つも無ければ、mism は、他のバージョンを選ばずに処理を中止してエラーを報告する (inst-2、dinst-2)。
8. mism は、選んだファイルの取得元の指定を、`modrinth:`、識別子、`version` の値、手順 5 で飛ばさなかった属性の値を、使った順に `:` でつないで作る (plg-12、dp-6)。`--lazy` で選んだときは、mism は、取得元の指定を手順 9 で作る。取得元の指定に書く値のどれかが `:` を含むとき、mism は、その値を取得元の指定に書けないことを伝えるエラーを報告する。
9. `--lazy` で選んだとき、mism は、手順 1 の候補をすべて候補として、手順 3 から手順 5 を尋ねずにやり直し、やり直しで使った値を手順 8 の形でつないで取得元の指定を作る (plg-12、dp-6)。やり直しでは、mism は、`version` の値を、選んだファイルが属する Modrinth のバージョンの `version_number` とする。プラグインでは、mism は、`software` の値を、手順 5 で決めた値があればその値とし、無ければ対象のサーバーのサーバーソフトウェアとする。mism は、`minecraft_version` の値を、手順 5 で決めた値があればその値とし、無ければ対象のサーバーの Minecraft のバージョンとする。mism は、`release_type` と `file_name` の値を、選んだファイルの値とする。やり直しで候補が 1 つに定まらなければ、mism は、選んだファイルを指す取得元の指定を作れないことを伝えるエラーを報告する。

### 性質

- d-select-1. `--lazy` で選んだファイルについて mism が出力した取得元の指定を、利用者が `--lazy` を付けずに与えたとき、mism は、尋ねずに同じファイルに定める。e.g. Minecraft のバージョン 26.3 の Paper のサーバーで、2026 年 10 月 21 日の FastAsyncWorldEdit のバージョンの一覧から `--lazy` で `modrinth:fastasyncworldedit` を選んだとき、mism が出力する取得元の指定は `modrinth:fastasyncworldedit:2.16.1:paper` である。
- d-select-2. 取得元の指定に書く値が `:` を含むとき、mism は、エラーを報告する。
- d-select-3. どの属性でも区別できないファイルが 2 つ以上残ったとき、mism は、エラーを報告する。
- d-select-4. プラグインの Modrinth のバージョンがファイルを 1 つも持たないとき、mism は、エラーを報告する。
- d-select-5. データパックの Modrinth のバージョンが、`file_type` が `null` のファイル 1 つと、`file_type` が `optional-resource-pack` のファイルを持つとき、mism は、そのバージョンの番号だけを付けた取得元の指定から、尋ねずに `file_type` が `null` のファイルに定める。e.g. 2026 年 10 月 21 日の Navas ZA Megas のバージョンの一覧では、mism は、`modrinth:navas-zamega:1.2.1` から `ZA_ENfor1.7.zip` に定める。
- d-select-6. 同じバージョンの番号を持つ Modrinth のバージョンが、`loaders` が `datapack` のものとモッドのものの 2 つであるとき、mism は、データパックについては、そのバージョンの番号だけを付けた取得元の指定から、尋ねずに `loaders` が `datapack` のバージョンのファイルに定める。e.g. 2026 年 10 月 21 日の True Ending のバージョンの一覧では、mism は、`modrinth:true-ending:1.1.4` から `true-ending-v1.1.4-1.21.2-5.zip` に定める。

### 判断

- 筆者は、Modrinth のバージョンがファイルを複数持てることと、同じ `version_number` の Modrinth のバージョンが 1 つのプロジェクトに複数あることを、2026 年 10 月 20 日と 21 日に Modrinth の API のドキュメントと API のレスポンスで確認した。e.g. FastAsyncWorldEdit の `2.16.1` は、`loaders` が `paper` のものと `spigot` のものの 2 つの Modrinth のバージョンを持ち、FastAsyncWorldEdit の `2.14.1` は、1 つの Modrinth のバージョンに 2 つのファイルを持つ。筆者は、この 2 つの例を 2026 年 10 月 21 日に確認した。
- `--lazy` で選んだときに mism が取得元の指定を手順 9 で作り直すのは、inst-2 が候補を 1 つの Modrinth のバージョンのファイルだけにするので、同じバージョンの番号を持つ別の Modrinth のバージョンがあると、手順 8 の取得元の指定では plg-12 の 2 番目の文 (その取得元の指定が尋ねずに同じファイルに定まること)が成り立たないからである。e.g. FastAsyncWorldEdit の `2.16.1` の `paper` のファイルについて、手順 8 の取得元の指定 `modrinth:fastasyncworldedit:2.16.1` は 2 つのファイルを指す。手順 9 で作った `modrinth:fastasyncworldedit:2.16.1:paper` は、そのファイルだけを指す。
- mism が一覧の属性の値の種類を一覧ごとに数えるのは、1 つの Modrinth のバージョンが 2 つのファイルを持つとき (e.g. FastAsyncWorldEdit の `2.14.1`)に、候補を減らさない `software` と `minecraft_version` の値を尋ねないためである (plg-10)。
- mism が `version` の値を、同じ番号を持つ Modrinth のバージョンのうち最も新しい公開日時で並べるのは、plg-11 がバージョンの番号を公開日時の新しい順に並べると定め、同じ番号を持つ Modrinth のバージョンが複数あり得るからである。
- mism が `:` を含む値をエラーにするのは、`:` を含む値を書いた取得元の指定を、mism が plg-3 の解析で別の値の並びとして読むからである。
- mism が尋ねる処理をトレイトとして受け取るのは、絞り込みの手順を、標準入力を使わずに単体テストするためである。
- 仕様書の dp-5 がデータパックで `software` を使わないのは、データパックの候補の `loaders` が必ず `datapack` を含み、その一覧の違い (e.g. `datapack` と `datapack,minecraft`)が、利用者がデータパックのファイルを選ぶ手がかりにならないからである。筆者は、`loaders` が `datapack,minecraft` の Modrinth のバージョンがあることを、2026 年 10 月 21 日に確認した。
- 筆者は、データパックについても、1 つの Modrinth のバージョンが同じ名前のファイルを 2 つ持つ例 (e.g. MissingMons [cobblemon] (識別子 `missingmons-cobblemon`)の `1.9` が持つ 2 つの `MissingMons [v1.9].zip`)を、2026 年 10 月 21 日に確認した。この 2 つのファイルは、どの属性でも区別できないので、mism は、手順 5 のとおりエラーを報告する。

## 標準入力からの回答の読み取り

### 責務

`prompt.rs` は、問いを書いて標準入力から改行までの回答を 1 つ読み、標準入力が端末かどうかを判定する。
このモジュールは、仕様書の fetch-2、dfetch-2、plg-11、inst-5、dinst-5 の回答の読み取りと、plg-11、inst-7、dinst-7 の「標準入力が端末である」かの判定を担う。

### 方法

mism は、標準入力が端末かどうかを、`std::io::IsTerminal` で判定する。

mism は、回答を次の手順で読む。

1. mism は、問いを `output.rs` を通して書く。
2. mism は、標準入力 (ファイル記述子 0)が読めるようになるまで、`poll` で最大 100 ミリ秒ずつ待つ。mism は、`poll` が戻るたびに (シグナルで中断されて戻ったときも)中断のフラグを確認し、フラグが立っていれば中断のエラーを返す。
3. 標準入力が読めるようになったら、mism は、ファイル記述子 0 から 1 バイトを読む。mism は、改行 (`\n`)を読むか、ファイルの終わりに達するまで、手順 2 と手順 3 を繰り返す。
4. mism は、読んだバイト列から末尾の改行を除き、UTF-8 の文字列として回答とする。読んだバイト列が UTF-8 でなければ、mism は、エラーを報告する。1 バイトも読まずにファイルの終わりに達したとき、回答は空である。
5. 標準入力と標準出力の両方が端末であるときを除き、mism は、回答を読んだ後に、標準出力に改行 1 つを書く。両方が端末であるときは、端末が利用者の入力した改行を標準出力に表示するので、mism は、改行を書かない。

### 性質

- d-prompt-1. mism が回答を待っている間に `SIGINT` を受けたとき、mism は、回答の入力を待たずに終了ステータス 130 で終了する。
- d-prompt-2. 標準入力が端末であり、利用者が 2 つの問いへの回答を先に続けて入力したとき、mism は、1 つの問いにつき、改行までの入力を 1 つだけ回答として読む。

### 判断

- 筆者は、signal-hook-registry 1.4.8 が `SA_RESTART` を付けてシグナルのハンドラを登録することを、2026 年 10 月 21 日にソースコードで確認した。`SA_RESTART` を付けたハンドラがシグナルを受けると、端末からの `read` はカーネルがやり直すので、利用者が改行を入力するまで戻らない。一方、Linux の `poll` は、`SA_RESTART` があってもシグナルで中断されて `EINTR` を返す。筆者は、このことを Linux の man ページ signal(7) で 2026 年 10 月 21 日に確認した。そのため、mism は、`poll` で待ち、`poll` が戻るたびに中断のフラグを確認する。
- mism が `poll` を最大 100 ミリ秒で区切るのは、mism がフラグを確認してから `poll` を始めるまでの間にシグナルが届くと、`poll` が中断されず、mism が次の入力まで中断に気付かないからである。100 ミリ秒は、mism がサーバーの終了を待つ間にフラグを確認する間隔 (「mism new の手順と中断」の節)と同じである。
- mism が回答を読んだ後に改行を書くのは、問いの文字列が改行で終わらないので、改行を書かなければ、次に書く文字列が問いの文字列の直後に改行無しで続くからである。標準入力と標準出力の両方が端末であるときに mism が改行を書かないのは、端末が入力の改行を表示した上に mism が改行を書くと、空行が 1 つ表示されるからである。
- mism が標準ライブラリの `Stdin` を使わずにファイル記述子 0 から 1 バイトずつ読むのは、`Stdin` が内部にバッファを持ち、1 つの回答より多くのバイトをバッファに読み込むと、`poll` が見るファイル記述子の状態とバッファの中身が食い違うからである。

## jar ファイルの宣言の読み取り

### 責務

`plugin_jar.rs` は、jar ファイルの中のプラグインの宣言を読む。
このモジュールは、仕様書の insp-1 の宣言の読み取りと、insp-4 の「宣言を持たない」かの判定と、insp-3 の `missing` と `conflict` の判定に使う `plugins/` の jar ファイルの宣言の読み取りを担う。

### 方法

仕様書が jar ファイルの宣言と呼ぶもの (insp-1)は、jar ファイル (zip 形式)の中の `plugin.yml` であり、`plugin.yml` が無ければ `paper-plugin.yml` である。mism は、zip の中のファイルのうち、パスが `plugin.yml` または `paper-plugin.yml` と完全に一致するもの (zip の最上位にあるもの)だけを宣言として探す。どちらも無ければ、jar ファイルは宣言を持たない (insp-4)。

mism は、jar ファイルの宣言を UTF-8 の文字列として読み (先頭に BOM があれば除き)、serde-saphyr で YAML として解析し、次のキーを読む。mism は、それ以外のキーを読まない。

- プラグインの名前は `name`、プラグインのバージョンは `version` であり、mism は、どちらも YAML に書かれたとおりの文字列として読む。
- 必要とする最小の Minecraft のバージョンは `api-version` であり、mism は、YAML に書かれたとおりの文字列として読む。
- Folia への対応は `folia-supported` であり、mism は、真偽値として読む。このキーが無ければ、宣言は Folia への対応を示していない。
- `plugin.yml` では、必須の依存先は `depend`、任意の依存先は `softdepend` であり、mism は、どちらも文字列の配列として読む。キーが無いか、値が空 (`null`)であれば、mism は、その依存先が無いとする。
- `paper-plugin.yml` では、依存先は `dependencies` の `bootstrap` と `server` の両方の要素である。`bootstrap` と `server` は、依存先の名前をキーとするマッピングであり、mism は、各依存先の `required` を真偽値として読む。`required` が真 (省略すれば真)の要素が必須の依存先、偽の要素が任意の依存先である。同じ名前が必須の依存先と任意の依存先の両方に現れれば、mism は、その名前を必須の依存先だけに数える。
- jar ファイルが `paper-plugin.yml` を持ち `plugin.yml` を持たないとき、宣言は Paper 専用のプラグインの形式である (insp-2 の `paper`)。

宣言が `name`、`version`、`api-version` を持たなければ、mism は、その値が無いとする。
jar ファイルが zip 形式でないとき、宣言のファイルが UTF-8 でないとき、宣言が YAML として解析できないとき (同じキーが 2 回あるマッピングを含む)、上のキーの値の型が合わないとき (e.g. `depend: Vault` のように配列でないとき)、mism は、jar ファイルのパスと原因を含むエラーを報告する。

### 性質

- d-jar-1. `plugin.yml` が引用符の無い `api-version: 1.20` と `version: 2.0` を持つとき、mism は、最小の Minecraft のバージョンを `1.20` と、プラグインのバージョンを `2.0` と出力する。
- d-jar-2. jar ファイルが `plugin.yml` と `paper-plugin.yml` の両方を持つとき、mism は、`plugin.yml` を宣言として読む。
- d-jar-3. `paper-plugin.yml` の `bootstrap` が `required` の無い依存先 `A` を持ち、`server` が `required: false` の依存先 `B` を持つとき、`A` は必須の依存先であり、`B` は任意の依存先である。
- d-jar-4. `paper-plugin.yml` が依存先 `A` を `bootstrap` に `required: false` で持ち、`server` に `required: true` で持つとき、`A` は必須の依存先にだけ現れる。
- d-jar-5. `plugin.yml` が `depend: Vault` (配列でない値)を持つとき、mism は、エラーを報告する。
- d-jar-6. jar ファイルが `plugin.yml` を zip の最上位ではなくディレクトリの中 (e.g. `META-INF/plugin.yml`)にだけ持つとき、jar ファイルは宣言を持たない。
- d-jar-7. mism が zip 形式でないファイルを検証するとき、mism は、宣言を持たないことを伝えるエラーとは別の、zip 形式でないことを伝えるエラーを報告する。

### 判断

- mism が `paper-plugin.yml` の `bootstrap` と `server` の両方を依存先に数えるのは、どちらも `plugins/` に無ければそのプラグインが動かない依存先であり、違いは使う時期だけだからである。
- 筆者は、`paper-plugin.yml` の `dependencies` の形を、2026 年 10 月 20 日と 21 日に Paper のドキュメントで確認した。
- mism が `plugin.yml` を `paper-plugin.yml` より先に読むのは、仕様書から設計書に移した決定である。Paper のドキュメントは、1 つの jar ファイルが両方を持てると書いているが、両方を持つ jar ファイルについてサーバーがどちらを使うかを書いていない。筆者は、このことを 2026 年 10 月 21 日に確認した (未決事項)。
- mism が `name`、`version`、`api-version` を書かれたとおりの文字列として読むのは、mism が宣言の値を YAML の型の規則で読むと、引用符の無い `1.20` が数の `1.2` になり、mism がプラグインの作者の書いた値と違う値を利用者に示すからである。筆者は、Paper が引用符の無い `api-version: 1.20` を `1.20` と読むか `1.2` と読むかを、確認していない (未決事項)。
- mism が値の型の合わない宣言をエラーにするのは、`docs/development-rules.md` の「コードの規則」の 6 番目 (フォールバックをしない)による。

## キャッシュのディレクトリのプラグインとデータパック

### 責務

`fetched_file_cache.rs` は、取得したファイルを、キャッシュのディレクトリの中の、呼び出す側のモジュールから名前を受け取ったディレクトリの下に保存し、そこから探す。`subcommands/plugin/` は、取得したプラグインの jar ファイルをキャッシュのディレクトリの `plugins/` の下に、`subcommands/datapack/` は、取得したデータパックのファイルをキャッシュのディレクトリの `datapacks/` の下に、このモジュールで保存する。
このモジュールは、仕様書の fetch-1 から fetch-4 と dfetch-1 から dfetch-4 の保存、place-1、insp-1、dplace-1、dinsp-1 の「その取得元の指定で取得したファイルがキャッシュのディレクトリにあるか」の判定、list-3、list-4、dlist-3、dlist-4 の一覧を担う。
この節の手順は、プラグインとデータパックで同じである。この節のパスの `<plugins|datapacks>` は、プラグインでは `plugins`、データパックでは `datapacks` である (「mism plugin と mism datapack の対象のサーバーと共通の手順」の節)。

### 方法

キャッシュのディレクトリの中のファイルのパスは、`<キャッシュのディレクトリ>/<plugins|datapacks>/<名前の / を - に置き換えたもの>/<ファイルの SHA-256>/<ファイルの名前>` である (e.g. `~/.cache/mism/plugins/LuckPerms/<64 桁の 16 進数>/LuckPerms-Bukkit-5.5.71.jar`、`~/.cache/mism/datapacks/True Ending - Ender Dragon Overhaul/<64 桁の 16 進数>/tru.e-ending-1.1.4d.zip`)。この設計書は、ファイルを直接持つ `<ファイルの SHA-256>` のディレクトリを、キャッシュのディレクトリの中の SHA-256 のディレクトリと呼ぶ。mism は、`<キャッシュのディレクトリ>/<plugins|datapacks>/`の下の 2 段目にあり名前が 64 桁の小文字の 16 進数であるディレクトリだけを SHA-256 のディレクトリとして扱い、`<キャッシュのディレクトリ>/<plugins|datapacks>/`の下のそれ以外のファイルとディレクトリを読まない。

SHA-256 のディレクトリは、取得したファイルと `source.toml` の 2 つのファイルを持つ。`source.toml` は、次の形の TOML である。

```toml
name = "LuckPerms"
source = "modrinth:luckperms:v5.5.71-bukkit"
file_name = "LuckPerms-Bukkit-5.5.71.jar"

[modrinth]
loaders = ["bukkit", "folia", "paper", "spigot"]
game_versions = ["1.8.9", "1.9", "26.3"]
required_dependencies = []
```

- `name` は、プラグイン (またはデータパック)の名前 (plg-5、dp-8)である。
- `source` は、そのファイルを取得したときの取得元の指定 (取得元が Modrinth なら plg-12 (または dp-6)の形、URL と手元のファイルなら利用者が与えた文字列)である。
- `file_name` は、ファイルの名前である。
- `modrinth` は、取得元が Modrinth のときだけあり、取得したファイルが属する Modrinth のバージョンの `loaders` と `game_versions` を Modrinth が返した順に持ち、必須の依存先の識別子 (「Modrinth の API」の節)を文字列の昇順に持つ。

mism は、`source.toml` を serde と toml で読み書きし、知らないフィールドがある場合、フィールドが欠けている場合 (`modrinth` を除く)、型が合わない場合に、ファイルのパスと原因を含むエラーを報告する。

mism は、取得したファイルを次の手順で保存する。

1. mism は、キャッシュのディレクトリが無ければそれを作り、キャッシュのディレクトリの直下に名前を持たないファイルを作る。
2. mism は、ファイルの内容を受け取りながら、それを名前を持たないファイルに書き、SHA-256 を計算する。取得元が Modrinth なら、mism は、SHA-512 も計算する。mism は、Modrinth と URL のファイルを `http_client.rs` でダウンロードし (fetch-1、fetch-2、dfetch-1、dfetch-2)、手元のファイルを読んで複製する (fetch-3、dfetch-3)。
3. 取得元が Modrinth で、計算した SHA-512 がファイルの `hashes.sha512` と一致しなければ、mism は、名前を持たないファイルを閉じ、処理を中止してエラーを報告する (fetch-1、dfetch-1)。このとき、キャッシュのディレクトリには何も増えていない (fetch-1、dfetch-1)。
4. mism は、中断のフラグを確認する。
5. mism は、`<キャッシュのディレクトリ>/<plugins|datapacks>/`の下のどれかのディレクトリの下に、計算した SHA-256 を名前とする SHA-256 のディレクトリがあるかを探す (fetch-4、dfetch-4)。2 つ以上あれば、mism は、それらのパスを含むエラーを報告する。1 つあれば、mism は手順 6 に進み、無ければ手順 7 に進む。
6. `fetched_file_cache.rs` は、既にある方の名前とファイルのパスを取得の手順 (`subcommands/plugin/fetch.rs` か `subcommands/datapack/fetch.rs`)に返し、取得の手順は、プラグインでは `mism: the jar is the same as the cached jar of <既にある方の名前>: <既にある方のファイルの絶対パス>`、データパックでは `mism: the datapack is the same as the cached datapack of <既にある方の名前>: <既にある方のファイルの絶対パス>` という文字列を書く (fetch-4、dfetch-4)。その後に、mism は、既にある方の `source.toml` の `source` と `modrinth` を今回の取得のものにして、`atomic_fs.rs` の置き換えで書き換え、名前を持たないファイルを閉じる (fetch-4、dfetch-4)。mism は、`name` と `file_name` を変えない。mism は、既にある方のファイルと名前を、この取得の結果とする (fetch-4、dfetch-4)。
7. mism は、`<キャッシュのディレクトリ>/<plugins|datapacks>/<名前の / を - に置き換えたもの>/` が無ければそれを作り、その中に作業用の名前のディレクトリを作る。mism は、名前を持たないファイルに、作業用の名前のディレクトリの中のファイルの名前を付け、同じディレクトリに `source.toml` を書く。mism は、作業用の名前のディレクトリの名前を、`renameat2` の `RENAME_NOREPLACE` で計算した SHA-256 に変える。その名前のディレクトリが既にあれば (同じ内容を同じ名前で同時に取得した別の mism がある)、mism は、作業用の名前のディレクトリを中身ごと削除し、手順 6 を行う。手順 7 のどこかが失敗したとき、mism は、作業用の名前のディレクトリを中身ごと削除し、エラーを報告する。

mism は、`<キャッシュのディレクトリ>/<plugins|datapacks>/`から、利用者が与えた取得元の指定のファイルを次のとおり探す (place-1、insp-1、dplace-1、dinsp-1)。

- Modrinth と URL の取得元の指定について、mism は、`source` が利用者の与えた文字列と完全に一致する `source.toml` を持つ SHA-256 のディレクトリを探す (place-1、dplace-1)。
- 手元のファイルの取得元の指定について、mism は、指定されたファイルの SHA-256 を計算し、その SHA-256 を名前とする SHA-256 のディレクトリを探す (place-1、dplace-1)。mism は、見つけたファイルを、その取得元の指定で取得したものと見なす。すなわち、mism は、名前を `source.toml` の `name` とし、取得元の指定を利用者が与えた文字列とし、Modrinth の申告を持たないものとして扱う。
- 1 つも見つからないことは、そのサブコマンドが扱う (place-1、insp-1、dplace-1、dinsp-1)。2 つ以上見つかれば、mism は、見つけたファイルのパスを含むエラーを報告する。

mism は、`<キャッシュのディレクトリ>/<plugins|datapacks>/`の下のすべての SHA-256 のディレクトリの `source.toml` を読んで、キャッシュのディレクトリにあるプラグイン (またはデータパック)の一覧 (list-3、dlist-3)を作る。`<キャッシュのディレクトリ>/<plugins|datapacks>/`が無ければ、一覧は空である。
このモジュールは、SHA-256 のディレクトリを削除しない (cache-6 と同じ方針)。

### 性質

- d-pcache-1. 取得の途中で mism がどのように終了しても、キャッシュのディレクトリに、取得したファイルと `source.toml` の片方だけを持つ SHA-256 のディレクトリは現れない。
- d-pcache-2. 同じ URL から内容の違う jar ファイルを 2 回取得した後に、利用者がその URL を `mism plugin place` に与えたとき、mism は、2 つの jar ファイルのキャッシュのディレクトリの中のパスを含むエラーを報告する。
- d-pcache-3. プラグインの名前が `A/B` のとき、その jar ファイルの SHA-256 のディレクトリは `<キャッシュのディレクトリ>/plugins/A-B/` の中にあり、その `source.toml` の `name` は `A/B` である。
- d-pcache-4. 同じ内容のファイルを `mism plugin fetch` と `mism datapack fetch` で取得したとき、mism は、`<キャッシュのディレクトリ>/plugins/` と `<キャッシュのディレクトリ>/datapacks/` のそれぞれに SHA-256 のディレクトリを 1 つずつ作る。

### 判断

- mism がキャッシュのディレクトリの中でファイルを SHA-256 のディレクトリに置くのは、同じ名前のファイルを別の取得元から取得しても互いを上書きしないためである。
- mism がプラグインとデータパックを別のディレクトリ (`plugins/` と `datapacks/`)に置き、fetch-4 と dfetch-4 の同じ SHA-256 のファイルを同じディレクトリの中だけで探すのは、`mism plugin list --global` と `mism datapack list --global` がそれぞれの種類のファイルだけを示し、`mism plugin place` が取得元の指定でデータパックのファイルを見つけないようにするためである (d-pcache-4)。
- mism が名前を持たないファイルをキャッシュのディレクトリの直下に作るのは、SHA-256 を計算し終えるまで SHA-256 のディレクトリの場所が決まらず、fetch-1 と dfetch-1 の照合が失敗したときにキャッシュのディレクトリに何も残さないために、照合の前にディレクトリを作らないからである。名前を付ける操作は同じファイルシステムの中でしか成り立たないので、mism は、キャッシュのディレクトリとその下のディレクトリが同じファイルシステムにあることを前提とし、そうでなければ名前の付与の失敗としてエラーを報告する。
- mism が SHA-256 のディレクトリを作業用の名前のディレクトリの中で作ってから名前を変えるのは、mism が取得したファイルと `source.toml` を SHA-256 のディレクトリに 2 回に分けて置くと、その間に止まった mism が片方だけを持つディレクトリを残すからである (d-pcache-1)。mism が `SIGKILL` で止められると作業用の名前のディレクトリが残ることがあるが、その名前は 64 桁の 16 進数ではないので、mism はそれを読まない。
- mism が Modrinth の申告を `source.toml` に記録するのは、`place` が Modrinth に問い合わせずに検証の記述 (insp-2、dinsp-2)を出力するからである (place-1、place-6、dplace-1、dplace-6)。mism は、Modrinth の申告のうちリリースの種別と公開日時を記録しない。キャッシュのディレクトリのファイルについてそれらを出力するサブコマンドが無いからである。
- fetch-4 (または dfetch-4)で mism が `source.toml` の `source` を置き換えた後も、手元のファイルの取得元の指定では mism が置き換えたファイルを見つけるのは、fetch-4 と place-1 (dfetch-4 と dplace-1)が、手元のファイルの取得元の指定について同じ SHA-256 のファイルを見つけると定めているからである。
- 取得元の指定のファイルが 2 つ以上あるときに mism がどれかを選ばずにエラーを報告するのは、どれを選ぶかの規則を仕様書が定めていないからである。同じ URL から内容の違うファイルを取得すると、この状態になる。利用者は、キャッシュのディレクトリの中のファイルのパスを手元のファイルの取得元の指定として与えれば、そのうちの 1 つを指定できる。

## プラグインの検証の値と判定

### 責務

`plugin_compatibility.rs` は、プラグインの検証の記述 (insp-2)の `compatibility` で始まる値とその出どころを作り、Modrinth の申告と検証の値を対象のサーバーと照らし合わせ (vers-3)、プラグインの検証の判定 (insp-3)を作る。
このモジュールは、仕様書の insp-2、insp-3、vers-3 を満たす。

### 方法

mism は、プラグインの jar ファイル 1 つについて、次の値を 1 つの構造体で持つ。検証の記述 (insp-2)の 13 個の値は、この構造体の値である。

- `file` は、プラグインの名前、Modrinth のバージョンの番号 (取得元が Modrinth でなければ無い)、取得元の指定の文字列 (取得元が Modrinth なら、`modrinth:` と識別子に属性の値を `:` でつないだ plg-12 の形)、jar ファイルの名前、jar ファイルの SHA-256、キャッシュのディレクトリの中の jar ファイルの絶対パスを持つ。
- `declaration` は、宣言から読んだ、名前、バージョン、最小の Minecraft のバージョン、Folia への対応、必須の依存先、任意の依存先、Paper 専用のプラグインの形式であるかを持つ。
- `compatibility` は、mism が `declaration` と、`source.toml` に記録した Modrinth の申告 (対応するサーバーソフトウェア、対応する Minecraft のバージョン、必須の依存先の識別子。取得元が Modrinth でなければ無い)から作る 6 つの値であり、その各値は、値と出どころ (`jar`、`modrinth`、その両方、無し)を持つ。

mism は、`compatibility` の 6 つの値を、次のとおり作る (insp-2)。

- `minimum_minecraft_version` は、宣言の `api-version` である。宣言が `api-version` を持てば出どころは `jar` であり、宣言が `api-version` を持たなければ、この値はどの情報源からも分からない。
- `minecraft_versions` は、Modrinth の申告の `game_versions` である。取得元が Modrinth であり `game_versions` が空でなければ出どころは `modrinth` であり、そうでなければこの値は分からない。
- `software` は、宣言から分かるサーバーソフトウェアと、Modrinth の申告の `loaders` を合わせた一覧である。宣言から分かるサーバーソフトウェアは、宣言の `folia-supported` が真なら `folia`、宣言が Paper 専用のプラグインの形式なら `paper` である (insp-2)。
- `folia_supported` は、宣言の `folia-supported` の値 (キーが無ければ偽)であり、出どころは `jar` である。
- `required_dependencies` は、宣言の必須の依存先の名前と、Modrinth の申告の必須の依存先の識別子を合わせた一覧である。
- `optional_dependencies` は、宣言の任意の依存先の名前の一覧である。

一覧の値の各要素の出どころは、その要素を含む情報源 (宣言と Modrinth の申告)の組である。同じ文字列が宣言と Modrinth の申告の両方にあれば、mism は、それを 1 つの要素とし、その要素の出どころを `jar,modrinth` とする。
要素が 1 つ以上ある一覧の値の出どころは、各要素の出どころを合わせたものである (insp-2)。要素が 1 つも無い一覧の値の出どころは、その一覧について述べている情報源の組である。宣言は、必須の依存先と任意の依存先について常に述べており (キーが無ければ依存先は無い)、`folia-supported` が真であるか Paper 専用のプラグインの形式であるときだけ、対応するサーバーソフトウェアについて述べている。Modrinth の申告は、取得元が Modrinth なら必須の依存先について常に述べており、`loaders` が空でなければ対応するサーバーソフトウェアについて述べている。
どの情報源からも分からない値を、mism は、`unknown` と出力し、その出どころを `-` と出力する (insp-2)。

mism は、検証の記述を、13 個の値について 1 つずつ、仕様書の insp-2 の順に書く (insp-2)。`compatibility` で始まらない 7 つの値の記述は、値の名前、タブ 1 文字、値を並べたものであり、`compatibility` で始まる 6 つの値の記述は、値の名前、タブ 1 文字、値、タブ 1 文字、出どころを並べたものである。`compatibility.folia_supported` の値は、`true` か `false` である。次の例は、URL から取得した架空のプラグインの検証の記述の形を示す。

```
mism: name	Example
mism: version	-
mism: source	https://example.com/Example.jar
mism: file.name	Example.jar
mism: file.sha256	<64 桁の 16 進数>
mism: declaration.name	Example
mism: declaration.version	1.0
mism: compatibility.minimum_minecraft_version	1.21	jar
mism: compatibility.minecraft_versions	unknown	-
mism: compatibility.software	folia	jar
mism: compatibility.folia_supported	true	jar
mism: compatibility.required_dependencies	Vault	jar
mism: compatibility.optional_dependencies	-	jar
```

mism は、対応するサーバーソフトウェアの一覧と対応する Minecraft のバージョンの一覧を、対象のサーバーと次のとおり照らし合わせる (vers-3)。

- 対応するサーバーソフトウェアの一覧が分からないとき (検証では `compatibility.software` が `unknown` のとき、`mism plugin versions` では `loaders` が空のとき)、結果は `unknown` である。
- 対応するサーバーソフトウェアの一覧が対象のサーバーのサーバーソフトウェアを含み、対応する Minecraft のバージョンの一覧が対象のサーバーの Minecraft のバージョンを含むとき、結果は `match` である。
- 対応するサーバーソフトウェアの一覧が対象のサーバーのサーバーソフトウェアを含み、対応する Minecraft のバージョンの一覧が対象のサーバーの Minecraft のバージョンを含まないとき (その一覧が分からないときを含む)、結果は `software-only` である。
- 対応するサーバーソフトウェアの一覧が対象のサーバーのサーバーソフトウェアを含まないとき、結果は `mismatch` である。

mism は、一覧が値を含むかを、文字列の完全な一致で決める。

mism は、対象のサーバーがあるとき、検証の判定を次のとおり作る (insp-3)。

1. `declared` の値は、`compatibility.software` と `compatibility.minecraft_versions` を照らし合わせた結果である。mism は、`declared` を常に出力する。
2. `incompatible` の理由は、次の 2 つである。1 つ目は、`compatibility.minimum_minecraft_version` があり、「Minecraft のバージョンの新旧の判定」の節の比較で、それが対象のサーバーの Minecraft のバージョンより新しいことであり、mism は、この理由を `requires Minecraft <最小の Minecraft のバージョン> or newer` と書く。最小の Minecraft のバージョンが解析できなければ、mism は、その文字列と jar ファイルのパスを含むエラーを報告する。2 つ目は、対象のサーバーが Folia であり、`compatibility.folia_supported` が偽であることであり、mism は、この理由を `does not declare Folia support` と書く。どちらかの理由があれば、mism は、`incompatible` を、理由の一覧とともに出力する。
3. `missing` の依存先は、`compatibility.required_dependencies` の要素のうち、対象のサーバーに無いものである。出どころに `jar` を含む要素は、`plugins/` の直下の jar ファイル (「プラグインとデータパックを置くディレクトリ」の節)のどれかの宣言の `name` がその要素と一致すれば、対象のサーバーにある。出どころに `modrinth` を含む要素は、`mism.toml` のプラグインの記録のどれかの取得元の指定の識別子 (「取得元の指定」の節)がその要素と一致すれば、対象のサーバーにある (insp-3)。対象のサーバーに無い要素があれば、mism は、`missing` を、その要素の一覧とともに出力する。
4. `conflict` の jar ファイルは、`plugins/` の直下の jar ファイルのうち、宣言の `name` が、検証する jar ファイルの宣言の `name` と一致するものである。検証する jar ファイルの宣言が `name` を持たなければ、mism は、この判定を行わない。そのような jar ファイルがあれば、mism は、`conflict` を、それらの jar ファイルの名前の一覧とともに出力する。

mism は、検証の判定の記述を、判定の名前、タブ 1 文字、値を並べたものとし、`declared`、`incompatible`、`missing`、`conflict` の順に、該当する判定につき 1 つずつ、改行で区切って書く (insp-3)。

### 性質

- d-compat-1. 取得元が URL であり、宣言が `depend` を持たないとき、mism は、`compatibility.required_dependencies` の値を `-` と、その出どころを `jar` と出力する。
- d-compat-2. 宣言の必須の依存先の名前と Modrinth の申告の必須の依存先の識別子が同じ文字列のとき、その文字列は、`compatibility.required_dependencies` の一覧に 1 度だけ現れる。
- d-compat-3. 対象のサーバーがあり、宣言の `api-version` が Minecraft のバージョンとして解析できないとき、`mism plugin inspect` は、エラーを報告する。
- d-compat-4. 対象のサーバーが無く、宣言の `api-version` が Minecraft のバージョンとして解析できないとき、`mism plugin inspect` は、その文字列を `compatibility.minimum_minecraft_version` の値として出力し、終了ステータス 0 で終了する。

### 判断

- mism が要素の無い一覧の出どころを、その一覧について述べている情報源とするのは、仕様書の insp-2 が一覧の出どころを要素の出どころから決めると定め、要素の無い一覧を定めていないからである。宣言が `depend` を持たないことは、必須の依存先が無いことを宣言が述べているのであり、mism にとって分からない値ではない。
- mism は、`folia_supported` の値を、常に宣言から作る。仕様書から設計書に移した決定は、宣言が無いときに Modrinth の申告から作ると定めていたが、mism は宣言を持たない jar ファイルの検証を中止する (insp-4)ので、宣言が無いときの検証の記述は無い。
- mism が一覧が値を含むかを文字列の完全な一致で決めるのは、sel-2 が Minecraft のバージョンの文字列を別の表記に変換しないと定めているのと揃えるためである。
- mism が解析できない `api-version` をエラーにするのは、それが `incompatible` の判定を左右し、`docs/development-rules.md` の「コードの規則」の 6 番目 (フォールバックをしない)があるからである。mism が Modrinth の申告の解析できない Minecraft のバージョンを除くのは、それらが Modrinth の Minecraft のバージョンの一覧に載っている正しい値だからである (「Minecraft のバージョンの新旧の判定」の節)。
- mism は、`missing` の判定で、宣言の `provides` (別の名前のプラグインの代わりになることの宣言)を見ない。仕様書の insp-3 が、依存先がサーバーにあることを「その名前を宣言していれば」と定めているからである。mism は、名前を大文字と小文字を区別して比べる。筆者は、Paper が依存先の名前の大文字と小文字を区別するかを確認していない (未決事項)。
- mism が `conflict` の判定で、検証する jar ファイルと同じ内容の jar ファイルも数えるのは、その jar ファイルを別の名前で `plugins/` に置くと、サーバーが同じ名前のプラグインを 2 つ見つけるからである。

## データパックの検証の値と判定

### 責務

`datapack_compatibility.rs` は、データパックの検証の記述 (dinsp-2)の値を作り、Modrinth の申告とデータパックの宣言の形式の番号の範囲を対象のサーバーと照らし合わせ (dvers-3)、データパックの検証の判定 (dinsp-3)を作る。
このモジュールは、仕様書の dinsp-2、dinsp-3、dvers-3 を満たす。

### 方法

mism は、データパックのファイル 1 つについて、検証の記述 (dinsp-2)の 9 個の値を、次のとおり作る。

- mism は、`name`、`version`、`source`、`file.name`、`file.sha256` を、「プラグインの検証の値と判定」の節のプラグインの同じ名前の値と同じ方法で作る (dinsp-2)。
- `declaration.min_format` と `declaration.max_format` は、`datapack_zip.rs` が `pack.mcmeta` から読んだ形式の番号の下限と上限である (「データパックのファイルの宣言の読み取り」の節)。宣言がそれを示さなければ、mism は、その 2 つを `unknown` と出力する (dinsp-2)。
- `modrinth.minecraft_versions` は、`source.toml` に記録した Modrinth の申告の `game_versions` であり、`modrinth.required_dependencies` は、`source.toml` に記録した必須の依存先の識別子である。取得元が Modrinth でなければ、mism は、その 2 つを `unknown` と出力する (dinsp-2)。

mism は、検証の記述を、9 個の値について 1 つずつ、仕様書の dinsp-2 の順に、値の名前、タブ 1 文字、値を並べた形で書く (dinsp-2)。データパックの検証の記述は、出どころを持たない。次の例は、Modrinth から取得した True Ending の `1.1.4d` の検証の記述の形を示す。

```
mism: name	True Ending - Ender Dragon Overhaul
mism: version	1.1.4d
mism: source	modrinth:true-ending:1.1.4d
mism: file.name	tru.e-ending-1.1.4d.zip
mism: file.sha256	<64 桁の 16 進数>
mism: declaration.min_format	71
mism: declaration.max_format	9999
mism: modrinth.minecraft_versions	1.21.2,1.21.3,1.21.4,1.21.5,1.21.6,1.21.7,1.21.8,1.21.9,1.21.10,1.21.11,26.1,26.1.1,26.1.2
mism: modrinth.required_dependencies	-
```

mism は、対象のサーバーがあるとき、データパックの検証の判定を次のとおり作る (dinsp-3)。

1. `declared` の値は、`modrinth.minecraft_versions` が対象のサーバーの Minecraft のバージョンを文字列の完全な一致で含むとき `match`、含まないとき `mismatch`、取得元が Modrinth でないとき `unknown` である。mism は、`declared` を常に出力する。
2. `pack_format` の値は、対象のサーバーのデータパックの形式の番号 (「サーバーのデータパックの形式の番号」の節)が、宣言の下限と上限の間 (両端を含む)にあるとき `match`、無いとき `mismatch`、宣言が形式の番号を示さないとき `unknown` である。mism は、`pack_format` を常に出力する。mism は、形式の番号を、メジャー番号、マイナー番号の順に整数として比べる。mism は、整数で書かれた下限を、そのメジャー番号とマイナー番号 0 の組とみなし、整数で書かれた上限を、そのメジャー番号のすべてのマイナー番号を含むものとみなす (dinsp-2)。
3. `missing` の依存先は、`modrinth.required_dependencies` の要素のうち、`mism.toml` のプラグインの記録とデータパックの記録のどの取得元の指定の識別子 (「取得元の指定」の節)とも一致しないものである。そのような依存先があれば、mism は、`missing` を、その依存先の一覧とともに出力する。

mism は、データパックの検証の判定の記述を、`declared`、`pack_format`、`missing` の順に、出力する条件が成り立つ判定につき 1 つずつ、改行で区切って書く (dinsp-3)。

mism は、判定の記述の後に、`declared` の値が `mismatch` であれば `mism: warning: Modrinth does not list Minecraft <対象のサーバーの Minecraft のバージョン> for this version of the datapack` という警告を、`pack_format` の値が `mismatch` であれば `mism: warning: the datapack declares the data pack formats <下限> to <上限>, which do not include <対象のサーバーのデータパックの形式の番号> of the server` という警告を、この順に、`output.rs` の警告として書く (dinsp-6、out-5)。<下限> と <上限> は、dinsp-2 の `declaration.min_format` と `declaration.max_format` と同じ形であり、<対象のサーバーのデータパックの形式の番号> は、`<メジャー番号>.<マイナー番号>` の形 (e.g. `121.0`)である。データパックの検証の判定には、配置を止める判定 (プラグインの `incompatible` と `conflict` に当たるもの)が無い。

mism は、`mism datapack versions` の照らし合わせ (dvers-3)を、`declared` と同じ方法で、その Modrinth のバージョンの `game_versions` について行う。

### 性質

- d-compat-7. データパックの必須の依存先の識別子が、`mism.toml` のプラグインの記録の取得元の指定の識別子と一致するとき、mism は、その依存先を `missing` の判定に含めない。
- d-compat-8. 宣言の下限が `121`、上限が `121` であり、対象のサーバーのデータパックの形式の番号が `121.0` のとき、`pack_format` は `match` である。
- d-compat-9. 宣言の下限が `[121, 1]` (出力では `121.1`)であり、対象のサーバーのデータパックの形式の番号が `121.0` のとき、`pack_format` は `mismatch` である。
- d-compat-10. 宣言の上限が `120` であり、対象のサーバーのデータパックの形式の番号が `120.5` のとき、`pack_format` は `match` である。

### 判断

- mism がデータパックの `missing` の判定でプラグインの記録も見るのは、必須の依存先の識別子が Modrinth のプロジェクトの識別子であり、そのプロジェクトがプラグインかデータパックかを、mism が依存先の識別子からは区別しないからである。
- mism がデータパックの検証の判定で配置を止めないのは、Paper が、形式の番号の合わないデータパックも有効にするからである。プラグインの `incompatible` と `conflict` は、Paper がそのプラグインを読み込まない場合を示すので、mism は配置を止める。データパックの `pack_format` の `mismatch` は、Paper がそのデータパックを読み込まない場合を示さないので、mism は、利用者に示すだけにする (仕様書の「データパックの管理」の節の、利用者が判断するという文)。筆者は、Paper 26.3 (バージョン 209。そのデータパックの形式の番号は 121.0)が、`pack_format` が 48 のデータパックを有効にすることを、2026 年 10 月 21 日に確認した。
- 筆者は、整数で書かれた下限がマイナー番号 0 を、整数で書かれた上限がすべてのマイナー番号を表すという規則を、2026 年 10 月 21 日に Minecraft Wiki の pack.mcmeta の記事で確認した。筆者は、Minecraft のサーバーが形式の番号の範囲をこの規則で比べることを、サーバーの実装では確認していない (未決事項)。

## プラグインとデータパックを置くディレクトリ

### 責務

`server_directory/plugins.rs` は、プラグインを置くディレクトリ (対象のサーバーの `plugins/`)を決め、プラグインの検証の判定 (insp-3)のために、`plugins/` の jar ファイルを列挙してその宣言を読む。`server_directory/datapacks.rs` は、データパックを置くディレクトリ (対象のサーバーのワールドのディレクトリの `datapacks/`)を決める。`server_directory/placed_files.rs` は、呼び出す側のモジュールが決めた置くディレクトリに、キャッシュのディレクトリのファイルを置いて `mism.toml` に記録を足し、置くディレクトリのファイルと記録を削除し、記録と置くディレクトリのファイルを突き合わせる。`server_directory/placed_files.rs` は、置くディレクトリのパス、`unmanaged` のファイルを見分けるファイルの名前の末尾、`mism.toml` の記録の配列 (`plugins` か `datapacks`)を、`subcommands/plugin/` と `subcommands/datapack/` から受け取る。
これらのモジュールは、仕様書の dp-3 のワールドのディレクトリ、place-2、place-3、place-4、list-1、list-2、rm-1、rm-2、rm-3、dplace-2、dplace-3、dplace-4、dlist-1、dlist-2、drm-1、drm-2、drm-3 を満たし、配置と削除について plg-8 と dp-11 を満たす。

### 方法

プラグインを置くディレクトリは、対象のサーバーのディレクトリの `plugins/` である (`server_directory/plugins.rs`)。
データパックを置くディレクトリ (`server_directory/datapacks.rs`)は、対象のサーバーのワールドのディレクトリの `datapacks/` である。ワールドのディレクトリは、`level-name` の値 (「server.properties の読み取り」の節)が相対パスなら、対象のサーバーのディレクトリの絶対パスにその値をつないだパスであり、`level-name` の値が絶対パスなら、その値そのものである (dp-3)。mism は、`level-name` の値を正規化しない (e.g. mism は、`../outside-world` の `..` をそのまま使う)。mism は、データパックを置くディレクトリを使うサブコマンド (`place`、`install`、`remove`、`--global` の無い `list`)の初めに `level-name` の値を読み、mism がその値を読めなければ、通信もファイルの変更もせずに処理を中止してエラーを報告する (dp-3)。

この節の以下の方法は、`server_directory/placed_files.rs` の方法であり、プラグインとデータパックで同じである。

置くディレクトリの直下のファイルは、置くディレクトリの直下にあり、名前がファイルの名前の末尾 (プラグインでは `.jar`、データパックでは `.zip`)で終わり、シンボリックリンクをたどった先が通常のファイルであるものである。置くディレクトリが無ければ、置くディレクトリの直下のファイルは無い。
mism は、記録の `file_name` を、置くディレクトリのパスにつなぐ前に、`file_name.rs` のファイルの名前の検査に通す。

mism は、配置の前に次のことを確かめる (place-3、place-4、dplace-3、dplace-4)。

- `mism.toml` の、プラグイン (またはデータパック)の記録の配列に、配置するプラグイン (またはデータパック)の名前と完全に一致する名前の記録があれば、mism は、処理を中止してエラーを報告する (place-3、dplace-3)。
- mism は、`<置くディレクトリ>/<ファイルの名前>` をシンボリックリンクをたどらずに調べ (`lstat`)、ファイル、ディレクトリ、シンボリックリンクのどれかがあれば、処理を中止してエラーを報告する (place-4、dplace-4)。
- 置くディレクトリのパス (データパックでは、ワールドのディレクトリのパスとその中の `datapacks`)に、シンボリックリンクをたどった先がディレクトリでないものがあれば、mism は、処理を中止してエラーを報告する。

mism は、ファイルを次の手順で配置する (place-2、dplace-2、plg-8、dp-11)。

1. 置くディレクトリのパスの要素のうち無いディレクトリを、mism は、ルートディレクトリに近いものから順に作り、自分が作ったディレクトリを覚えておく (place-2、dplace-2)。プラグインでは、mism が作るディレクトリは `plugins/` だけである。データパックでは、mism は、ワールドのディレクトリが無ければワールドのディレクトリ (とその親のディレクトリのうち無いもの)も作る。
2. mism は、置くディレクトリの中に名前を持たないファイルを作り、キャッシュのディレクトリのファイルを読んでそのファイルに書きながら、SHA-256 を計算する。計算した SHA-256 が、そのファイルを持つキャッシュのディレクトリの中の SHA-256 のディレクトリの名前と違えば、mism は、名前を持たないファイルを閉じ、エラーを報告する。
3. mism は、名前を持たないファイルに、`<置くディレクトリ>/<ファイルの名前>` の名前を付ける (place-2、dplace-2)。その名前のものが既にあれば (mism が place-4 (または dplace-4)の条件を確かめた後に、別のプロセスがその名前のものを作ったとき)、mism は、place-4 (または dplace-4)と同じエラーを報告する。
4. mism は、`mism.toml` のプラグイン (またはデータパック)の記録の配列の末尾に、そのプラグイン (またはデータパック)の記録 (plg-4、dp-7)を足して、`mism.toml` を置き換えで書く (place-2、dplace-2)。
5. 手順 1 から手順 4 のどれかが失敗したとき、mism は、手順 3 で名前を付けたファイルがあればそれを削除し、手順 1 で作ったディレクトリを作った順と逆の順に削除してから、エラーを報告する (plg-8、dp-11)。

mism は、プラグイン (またはデータパック)を次の手順で削除する (rm-1、rm-2、rm-3、drm-1、drm-2、drm-3、plg-8、dp-11)。

1. mism は、`mism.toml` のプラグイン (またはデータパック)の記録の配列から、利用者が与えた名前と完全に一致する名前の記録を探す。無ければ、mism は、処理を中止してエラーを報告する (rm-2、drm-2)。2 つ以上あれば、mism は、処理を中止してエラーを報告する。
2. `<置くディレクトリ>/<記録の file_name>` が通常のファイルであれば、mism は、その名前を、`renameat2` の `RENAME_NOREPLACE` で、置くディレクトリの中の作業用の名前に変える。通常のファイルでなければ (rm-3、drm-3)、mism は、この手順を飛ばす。
3. mism は、`mism.toml` からその記録を除き、`mism.toml` を置き換えで書く (rm-1、rm-3、drm-1、drm-3)。書き込みが失敗したとき、mism は、手順 2 で変えた名前を元の名前に戻してから、エラーを報告する (plg-8、dp-11)。
4. mism は、手順 2 で作業用の名前に変えたファイルを削除する (rm-1、drm-1)。mism は、置くディレクトリの中のそれ以外のファイルとディレクトリを削除しない (rm-1、drm-1)。

mism は、記録と置くディレクトリの直下のファイルを次のとおり突き合わせる (list-1、list-2、dlist-1、dlist-2)。

1. mism は、`mism.toml` のプラグイン (またはデータパック)の記録の順に、各記録について状態を決める。記録の状態は、`<置くディレクトリ>/<記録の file_name>` が通常のファイルであり、その SHA-256 が記録の `sha256` と一致すれば `ok`、一致しなければ `changed`、`<置くディレクトリ>/<記録の file_name>` が通常のファイルでなければ `missing` である (list-2、dlist-2)。
2. mism は、どの記録の `file_name` とも一致しない名前の置くディレクトリの直下のファイルを、名前の昇順に並べ、それぞれの状態を `unmanaged` とする (list-2、dlist-2)。
3. 各記述は、状態、名前、バージョンの番号、ファイルの名前、取得元の指定を並べたものである (list-2、dlist-2)。`unmanaged` の記述では、名前はファイルの名前から `.jar` (または `.zip`)を除いたものであり、バージョンの番号と取得元の指定は `-` である (list-2、dlist-2)。

`server_directory/plugins.rs` は、`plugins/` の直下の各 jar ファイルの宣言を `plugin_jar.rs` で読む (insp-3)。`plugin.yml` も `paper-plugin.yml` も持たない jar ファイルについて、mism は、その jar ファイルがどのプラグインの名前も持たないものとして扱う。宣言が読めない jar ファイルがあれば、mism は、その jar ファイルのパスを含むエラーを報告する。

### 性質

- d-pdir-1. キャッシュのディレクトリの jar ファイルの SHA-256 が、その jar ファイルを持つキャッシュのディレクトリの中の SHA-256 のディレクトリの名前と違うとき、`mism plugin place` は、エラーを報告し、`plugins/` と `mism.toml` を変えない。
- d-pdir-2. `mism plugin list` の出力は、`mism.toml` の記録の順の記述の後に、`unmanaged` の記述を jar ファイルの名前の昇順に並べたものである。
- d-dpdir-1. `level-name` の値が `worlds/survival` であり、対象のサーバーのディレクトリに `worlds` が無いとき、`mism datapack place` が成功すると、`worlds/survival/datapacks/<ファイルの名前>` がある。
- d-dpdir-2. `level-name` の値が絶対パス `<ディレクトリ>/worlds/survival` であり、`<ディレクトリ>/worlds` が無く、`mism.toml` の置き換えが失敗したとき、`mism datapack place` の実行の後にも `<ディレクトリ>/worlds` は無い。
- d-dpdir-3. `<ワールドのディレクトリ>/datapacks` がファイルであるとき、`mism datapack place` は、エラーを報告し、ワールドのディレクトリと `mism.toml` を変えない。
- d-dpdir-4. `datapacks/` の直下にディレクトリ (展開されたデータパック)があるとき、`mism datapack list` は、そのディレクトリを出力しない。

### 判断

- mism が配置でファイルを置くディレクトリに置いてから記録を足すのは、記録を足す書き込みが失敗したときに、置いたファイルを削除すれば実行の前の状態に戻せるからである (plg-8、dp-11)。
- mism が削除でファイルを先に作業用の名前に変えるのは、置くディレクトリに書き込めないとき (e.g. `plugins/` の持ち主が別の利用者のとき)に、`mism.toml` を変える前に失敗するためである。mism が `mism.toml` を先に書き換えてからファイルを削除すると、削除の失敗で `mism.toml` だけが変わり、plg-8 と dp-11 に反する。mism が手順 2 で置くディレクトリの中の名前を変えられたなら、mism は同じ権限でその中のファイルを削除できるので、手順 4 が失敗することはほとんど無い。手順 4 が失敗したときは、mism は、残った作業用の名前のファイルのパスを含むエラーを報告する。
- mism が配置で複製した内容の SHA-256 を照合するのは、キャッシュのディレクトリのファイルが保存の後に書き換えられていても、記録の `sha256` が置くディレクトリのファイルの内容と一致するようにするためである。
- mism が配置で失敗したときに作ったディレクトリをすべて削除するのは、dp-11 が、実行の前にワールドのディレクトリのパスに何も無かったときは実行の後も何も無いと定めているからである。mism は、ワールドのディレクトリの親のディレクトリ (e.g. `worlds`)も、自分が作ったものなら削除する (d-dpdir-2)。
- mism が配置の前の確認で `lstat` を使うのは、「mism new の手順と中断」の節と同じく、`stat` がリンク先の無いシンボリックリンクを「存在しない」と報告するからである。
- mism が宣言の読めない `plugins/` の jar ファイルをエラーにするのは、`docs/development-rules.md` の「コードの規則」の 6 番目 (フォールバックをしない)による。mism がその jar ファイルを飛ばすと、mism は、その jar ファイルの宣言を見ないまま `missing` と `conflict` の判定を決めることになる。
- mism がデータパックの一覧で `datapacks/` の直下のディレクトリを `unmanaged` に含めないのは、仕様書の dlist-2 が `unmanaged` を名前が `.zip` で終わるファイルと定めているからである。筆者は、Paper が `datapacks/` の直下のディレクトリをデータパックとして読み込むかを確認していない (未決事項)。
- データパックの記録は、ファイルを置いたワールドのディレクトリを持たないので、mism は、記録のファイルを、そのときの `level-name` の値から決めたワールドのディレクトリの `datapacks/` の中で探す。
- 筆者は、2026 年 10 月 21 日に、Paper 26.3 (バージョン 209)のサーバーのディレクトリの複製を起動して、次のことを確認した。
  - Paper は、起動のたびに、ワールドのディレクトリの直下の `datapacks/` の中の zip 形式のデータパックを探し、新しく見つけたデータパックを、`Found new data pack file/<ファイルの名前>, loading it automatically` と出力して有効にする。ワールドのディレクトリが `datapacks/` だけを持つ状態 (ワールドがまだ生成されていない状態)で起動したときも、ワールドを生成した後に `datapacks/` にファイルを足して起動し直したときも、Paper はそのデータパックを有効にし、`datapack list` の結果は、そのデータパックを有効なものとして示した。そのため、mism は、ワールドのディレクトリが無ければそれを作ってデータパックを置く (dplace-2)。
  - `level-name` の値が `w2`、`worlds/survival`、`../outside-world`、絶対パスのどれであっても、Paper は、その値を、サーバーのディレクトリを基準にしたパス (絶対パスはそのまま)として、ワールドのディレクトリに使った。そのため、mism は、`level-name` の値の中の `/` と `..` と絶対パスを拒まない。
  - Paper は、ワールドのディレクトリとして `level-name` の値のディレクトリを 1 つだけ作り、その中に `dimensions/` を作った。
- 筆者は、同じ日に、Folia 26.3 (バージョン 4)も、`world/datapacks/` の中のデータパックを有効にすることを確認した。ただし、Folia は、veinminer のデータパックの関数のうち `scoreboard` のコマンドを使うものを、`Unknown or incomplete command` と出力して読み込まなかった。mism は、データパックが使うコマンドを確かめない。仕様書の「データパックの管理」の節は、mism がデータパックがサーバーで動くかを保証しないと定めている。

## server.properties の読み取り

### 責務

`server_directory/server_properties.rs` は、対象のサーバーのディレクトリの直下の `server.properties` を読み、`level-name` の値を取り出す。
このモジュールは、仕様書の dp-3 の `level-name` の値の読み取りと、dp-3 のエラーを満たす。

### 方法

mism は、`server.properties` を UTF-8 の文字列として読む。UTF-8 として読めないとき、mism は、`server.properties` のパスを含むエラーを報告する。
mism は、その文字列を、Java の `java.util.Properties` の `load` が読む形式として、次の 5 つの規則で読む。

1. mism は、文字列を改行 (`\n`、`\r\n`、`\r`)の位置で区切る。区切った部分のうち、規則 2 で前の部分につながるものを除く各部分について、その部分から先頭の空白 (空白、タブ、改ページ)を除いたものが空であるか、`#` か `!` で始まるとき、mism は、その部分を読まない。
2. mism が読む部分の末尾に連続する `\` の数が奇数であるとき、mism は、その部分の末尾の `\` を 1 つ除き、次の部分から先頭の空白を除いたものを、その後につなぐ。mism は、つないだ結果の末尾についても、この規則を繰り返す。
3. mism は、つないだ結果から先頭の空白を除き、残りを、エスケープされていない最初の `=`、`:`、空白のどれかの前までのキーと、その文字の後の値に分ける。分けた文字が空白であったとき、mism は、値の先頭の空白を除き、その後に `=` か `:` が 1 つあれば、それとその後の空白も除く。分けた文字が `=` か `:` であったとき、mism は、値の先頭の空白を除く。
4. mism は、キーと値の中のエスケープを戻す。`\t`、`\n`、`\r`、`\f` は、それぞれタブ、改行 (`\n`)、復帰 (`\r`)、改ページになる。`\u` に 4 桁の 16 進数が続くものは、その数の UTF-16 の符号単位になる。それ以外の `\` と 1 文字の組は、その 1 文字になる。`\u` の後が 4 桁の 16 進数でないとき、または `\u` が作る符号単位の並びが UTF-16 として正しくないとき、mism は、`server.properties` のパスを含むエラーを報告する。
5. 同じキーを持つ部分が 2 つ以上あるとき、mism は、最後の部分の値を使う。

mism は、キーが `level-name` と完全に一致する値を、`level-name` の値とする (dp-3)。`server.properties` が無いとき (「ファイルが存在しない」という理由で読み込みが失敗したとき)、`server.properties` が `level-name` を持たないとき、`level-name` の値が空のとき、mism は、処理を中止してエラーを報告する (dp-3)。

### 性質

- d-props-1. `server.properties` が `level-name=my\:world` を持つとき、mism が使う `level-name` の値は `my:world` である。
- d-props-2. `server.properties` が `level-name = world` を持つとき、mism が使う `level-name` の値は `world` である。
- d-props-3. `server.properties` が `#level-name=other` と `level-name=world` をこの順に持つとき、mism が使う `level-name` の値は `world` である。
- d-props-4. `server.properties` が、`\` で終わる `level-name=my\` と、その次の、先頭に空白を 2 つ持つ `  survival` を持つとき、mism が使う `level-name` の値は `mysurvival` である。
- d-props-5. `server.properties` が `level-name=caf\u00e9` を持つとき、mism が使う `level-name` の値は `café` である。
- d-props-6. `server.properties` が UTF-8 として読めないとき、`mism datapack install` は、Modrinth にリクエストを送らずにエラーを報告する。

### 判断

- 筆者は、Paper 26.3 (バージョン 209)が書いた `server.properties` が、先頭に `java.util.Properties` の `store` が書く形の 2 つの注釈 (`#Minecraft server properties` と、日時)を持ち、値の中の `:` を `\:` と書く (e.g. `level-type=minecraft\:normal`)ことを、2026 年 10 月 21 日に確認した。そのため、mism は、`server.properties` を `java.util.Properties` の形式で読み、`\:` のようなエスケープを戻す。
- 筆者は、Minecraft のサーバーが `server.properties` を `java.util.Properties` の `load` で読むことと、UTF-8 として読めない `server.properties` をどう読むかを、確認していない (未決事項)。mism が UTF-8 として読めない `server.properties` を別の文字コードで読み直さないのは、`docs/development-rules.md` の「コードの規則」の 6 番目 (フォールバックをしない)による。
- mism が `level-name` を持たない `server.properties` で既定の値 (e.g. `world`)を補わないのは、筆者がその場合にサーバーが使うワールドのディレクトリを確認しておらず、mism が補った値がサーバーの使う値と違うと、mism がデータパックをサーバーの読まないディレクトリに置くからである。`mism new` の初回起動で Paper 26.3 が書いた `server.properties` は、`level-name=world` を持つ。筆者は、このことを 2026 年 10 月 21 日に確認した。
- mism が空の `level-name` の値をエラーにするのは、空の値を対象のサーバーのディレクトリにつなぐとワールドのディレクトリが対象のサーバーのディレクトリそのものになり、筆者が、その場合にサーバーが使うワールドのディレクトリを確認していないからである。

## データパックのファイルの宣言の読み取り

### 責務

`datapack_zip.rs` は、データパックのファイルが zip 形式であり、zip の最上位に `pack.mcmeta` を持つかを判定し、`pack.mcmeta` (データパックの宣言)から、対応するデータパックの形式の番号の下限と上限を読む。
このモジュールは、仕様書の dinsp-4 の判定と、dinsp-2 の `declaration.min_format` と `declaration.max_format` の値を担う。

### 方法

mism は、zip のクレートでファイルを zip 形式として開き、zip の中のファイルのうち、パスが `pack.mcmeta` と完全に一致するもの (zip の最上位にあるもの)を探す。
zip 形式として開けないとき、mism は、そのファイルが zip 形式でないことを伝えるエラーを報告する (dinsp-4)。`pack.mcmeta` が無いとき、mism は、そのファイルが `pack.mcmeta` を持たないことを伝えるエラーを報告する (dinsp-4)。

mism は、`pack.mcmeta` を UTF-8 の文字列として読み (先頭に BOM があれば除き)、serde_json で JSON として解析し、`pack` の中の次のキーを読む。mism は、それ以外のキー (e.g. `description`、`overlays`)を読まない。

- `min_format` と `max_format` は、整数か、2 つの整数の配列である。
- `supported_formats` は、整数か、2 つの整数の配列か、`min_inclusive` と `max_inclusive` の 2 つの整数を持つマッピングである。
- `pack_format` は、整数である。

mism は、下限と上限を次の順に決める (dinsp-2)。

1. 宣言が `min_format` と `max_format` の両方を持てば、下限は `min_format`、上限は `max_format` である。
2. そうでなく、宣言が `supported_formats` を持てば、下限と上限は、`supported_formats` が整数ならその整数であり、配列なら 1 つ目と 2 つ目の整数であり、マッピングなら `min_inclusive` と `max_inclusive` である。
3. そうでなく、宣言が `pack_format` を持てば、下限と上限は `pack_format` である。
4. どれも持たなければ、宣言は形式の番号を示さない (dinsp-2 の `unknown`)。

mism は、下限と上限を、整数か 2 つの整数の組かを区別して持つ。出力では、mism は、整数を整数のまま、組を `<1 つ目>.<2 つ目>` の形で書く (dinsp-2)。
`pack.mcmeta` が UTF-8 でないとき、JSON として解析できないとき、`pack` を持たないとき、上のキーの値の型が合わないとき、mism は、データパックのファイルのパスと原因を含むエラーを報告する。`min_format` と `max_format` の片方だけを持つとき、mism は、エラーを報告する (dinsp-4)。

### 性質

- d-dpzip-1. データパックのファイルが `pack.mcmeta` をディレクトリの中 (e.g. `inner/pack.mcmeta`)にだけ持つとき、mism は、`pack.mcmeta` を持たないことを伝えるエラーを報告する。
- d-dpzip-2. データパックのファイルが zip 形式でないとき、mism は、`pack.mcmeta` を持たないことを伝えるエラーとは別の、zip 形式でないことを伝えるエラーを報告する。
- d-dpzip-3. `pack.mcmeta` が `"min_format": [121, 0]` と `"max_format": 121` を持つとき、mism は、下限を `121.0`、上限を `121` と出力する。
- d-dpzip-4. `pack.mcmeta` が `"pack_format": 71`、`"supported_formats": {"min_inclusive": 71, "max_inclusive": 9999}` を持ち、`min_format` と `max_format` を持たないとき、mism は、下限を `71`、上限を `9999` と出力する。

### 判断

- 筆者は、Paper 26.3 (バージョン 209)が、最上位に `pack.mcmeta` を持たない zip 形式のファイルを `datapacks/` の中に見つけると、`Missing metadata in pack file/<ファイルの名前>` と出力し、そのファイルをデータパックとして有効にしないことを、2026 年 10 月 21 日に確認した。
- 筆者は、`pack.mcmeta` の `min_format`、`max_format`、`supported_formats`、`pack_format` の形と、Minecraft 1.21.9 から `min_format` と `max_format` が使われ、それより前のバージョンにも対応するデータパックが `pack_format` と `supported_formats` も持つことを、2026 年 10 月 21 日に Minecraft Wiki の pack.mcmeta の記事で確認した。筆者は、宣言が 3 つのキーを同時に持つときにサーバーがどれを使うかを確認していない (未決事項)。mism が `min_format` と `max_format` を先に使うのは、それが Minecraft 1.21.9 からの書き方だからである。
- 筆者は、`pack.mcmeta` の書き方が一定でないこと (e.g. Terralith は `"min_format": 121` と `"max_format": 121` を、veinminer は `"min_format": [121, 0]` と `"max_format": 121` を、True Ending は `pack_format`、`min_format`、`max_format`、`supported_formats` のすべてを持ち、`description` は文字列のものと JSON の配列のものがある)を、それらのファイルで 2026 年 10 月 21 日に確認した。mism が `description` を読まないのは、仕様書の dinsp-2 が `description` を示す値を定めていないからである。

## サーバーのデータパックの形式の番号

### 責務

`server_directory/server_jar.rs` は、対象のサーバーの Minecraft のバージョンのデータパックの形式の番号を、対象のサーバーのディレクトリの中のサーバーの jar ファイルから読む。
このモジュールは、仕様書の dinsp-3 の `pack_format` の判定に使う、対象のサーバーのデータパックの形式の番号と、dinsp-5 のエラーを担う。

### 方法

mism は、`<対象のサーバーのディレクトリ>/versions/<Minecraft のバージョン>/<paper|folia>-<Minecraft のバージョン>.jar` (e.g. `versions/26.3/paper-26.3.jar`)を zip 形式として開き、その最上位の `version.json` を serde_json で読む。Minecraft のバージョンとサーバーソフトウェアは、`mism.toml` の `minecraft_version` と `software` の値である。
mism は、`version.json` の `pack_version` から、データパックの形式の番号を次のとおり読む。

1. `pack_version` が `data_major` と `data_minor` の 2 つの整数を持てば、形式の番号は、メジャー番号 `data_major` とマイナー番号 `data_minor` の組である。
2. そうでなく、`pack_version` が `data` の整数を持てば、形式の番号は、メジャー番号 `data` とマイナー番号 0 の組である。

jar ファイルが無いとき、zip 形式として開けないとき、`version.json` が無いか読めないとき、`pack_version` が上のどちらの形でもないとき、mism は、jar ファイルのパスと原因を含むエラーを報告する (dinsp-5)。
mism は、この番号を、`mism datapack inspect`、`mism datapack place`、`mism datapack install` が対象のサーバーについて検証の判定 (dinsp-3)を作るときだけ読む。

### 性質

- d-sjar-1. `versions/26.3/paper-26.3.jar` の `version.json` の `pack_version` が `{"data_major": 121, "data_minor": 0, ...}` のとき、mism が使う対象のサーバーのデータパックの形式の番号は `121.0` である。

### 判断

- 筆者は、`mism new` の初回起動の後の Paper 26.3 (バージョン 209)と Folia 26.3 (バージョン 4)のサーバーのディレクトリが、`versions/26.3/paper-26.3.jar` と `versions/26.3/folia-26.3.jar` を持ち、その最上位の `version.json` の `pack_version` が `"data_major": 121` と `"data_minor": 0` を持つことを、2026 年 10 月 21 日に確認した。この jar ファイルは、Paper と Folia が初回起動で作るものであり、mism は、`mism new` の初回起動 (new-4)でそれを作らせる。
- mism が形式の番号を mism の中の表 (Minecraft のバージョンと形式の番号の対応)で決めないのは、Minecraft の新しいバージョンが出るたびに表を直すことになるからである。mism が形式の番号を PaperMC の API から得ないのは、筆者が、PaperMC の API のレスポンスに形式の番号が無いことを、2026 年 10 月 20 日に確認したキーの一覧 (「PaperMC の API」の節)で確認しているからである。
- 筆者は、`pack_version` が `data` の整数を持つ形 (Minecraft 1.17 から 1.21.8 まで)と、1 つの整数である形 (Minecraft 1.17 より前)を、2026 年 10 月 21 日に Minecraft Wiki の version.json の記事で確認した。mism は、1 つの整数である形を読まずにエラーを報告する。mism がその形を読まないのは、その形の Minecraft のバージョンで、その整数がデータパックの形式の番号かリソースパックの形式の番号かを、筆者が確認していないからである。筆者は、Minecraft 1.21.8 以前の Paper の `versions/` の jar ファイルの `version.json` を、実物では確認していない (未決事項)。
- mism が `versions/` の下の jar ファイルを読み、対象のサーバーのディレクトリの直下の Paper (または Folia)の jar ファイルを読まないのは、`mism.toml` がその jar ファイルの名前を持たないからである。筆者は、直下の jar ファイル (e.g. `paper-26.3-209.jar`)も最上位に同じ `version.json` を持つことを、2026 年 10 月 21 日に確認した。

## mism plugin search と mism datapack search の手順

### 責務

`subcommands/plugin/search.rs` は `mism plugin search` の手順の全体を、`subcommands/datapack/search.rs` は `mism datapack search` の手順の全体を担う。`subcommands/modrinth_search.rs` は、2 つのモジュールが使う、この節の手順 1 から手順 4 を持つ。
この 2 つのモジュールは、仕様書の srch-1 から srch-5 と dsrch-1 から dsrch-5 を満たす。

### 方法

`subcommands/plugin/search.rs` はプラグインの値で、`subcommands/datapack/search.rs` はデータパックの値で (「Modrinth の API」の節の表)、次の手順を行う。

1. 利用者が `--limit` を省略したとき、mism は、件数を 10 とする (srch-1、dsrch-1)。利用者が `--limit` を与えたとき、mism は、その値が ASCII の数字だけからなり、整数として 10 以上 100 以下であることを確かめ、そうでなければ、通信せずに処理を中止してエラーを報告する (srch-5、dsrch-5)。
2. mism は、検索のエンドポイントを、プラグイン (またはデータパック)の `facets` の値で 1 回呼ぶ (srch-1、dsrch-1)。
3. mism は、レスポンスの `hits` の各要素について、`hits` の順に、バージョンの一覧のエンドポイントを 1 回ずつ呼ぶ (srch-2、dsrch-2)。
4. mism は、各プロジェクトの Modrinth のバージョンの一覧から、検索の記述に使う Modrinth のバージョン (プラグインでは `loaders` が `paper` か `folia` を含むもの、データパックでは `loaders` が `datapack` を含むもの)のうち、`date_published` が最も新しいものを選ぶ (srch-2、dsrch-2)。公開日時が最も新しいものが複数あれば、mism は、バージョンの一覧のエンドポイントが返した順で先のものを選ぶ (srch-2、dsrch-2)。
5. すべての通信が終わった後に、mism は、`hits` の順に、各プロジェクトの記述を、`output.rs` を通して 1 つずつ改行で区切って書き、終了ステータス 0 で終了する (srch-1、dsrch-1、plg-9、dp-12、out-1)。プラグインの記述は、`modrinth:` に続く `slug`、`title`、選んだ Modrinth のバージョンの `version_number`、`loaders`、`game_versions` のうち最も新しいもの (「Minecraft のバージョンの新旧の判定」の節)を並べたものであり (srch-2)、データパックの記述は、その 5 つから `loaders` を除いた 4 つを並べたものである (dsrch-2)。プロジェクトが検索の記述に使う Modrinth のバージョンを 1 つも持たなければ、mism は、`title` より後の値を `-` とする (plg-9、dp-12)。`hits` が空なら、mism は、何も書かずに終了ステータス 0 で終了する (srch-3、dsrch-3)。

通信のどれかが失敗したとき、mism は、手順 5 に進まないので標準出力にプロジェクトの記述を 1 つも書かず、検索を中止してエラーを報告する (srch-4、dsrch-4)。

### 性質

- d-search-1. `mism plugin search` と `mism datapack search` は、検索のエンドポイントを 1 回と、見つかったプロジェクトごとにバージョンの一覧のエンドポイントを 1 回だけ呼び、それ以外のエンドポイントを呼ばない。
- d-search-2. 見つかったプロジェクトのどれかのバージョンの一覧の取得に失敗したとき、mism は、標準出力にプロジェクトの記述を 1 つも書かない。

### 判断

- mism が見つかったプラグインごとにバージョンの一覧を得るのは、検索のレスポンスが、公開日時が最も新しい Modrinth のバージョンを示さないからである。検索のレスポンスの各要素は `latest_version` (Modrinth のバージョンの id)を持つが、筆者は、2026 年 10 月 21 日に、LuckPerms の `latest_version` が `v5.5.71-velocity` (公開日時 2026-08-06)を指し、LuckPerms の公開日時が最も新しい Modrinth のバージョンが `v5.5.87-neoforge` (公開日時 2026-10-06)であることを確認した。そのため、mism は `latest_version` を使わない。
- 件数が 100 のとき、mism は、Modrinth に 101 回のリクエストを送る。これは、Modrinth のリクエストの数の上限 (1 分間に 300 回)より少ない。
- mism が `loaders` に `paper` または `folia` を含む Modrinth のバージョンの中から選ぶのは、mism が扱うサーバーが Paper と Folia であり、LuckPerms のように他のサーバーソフトウェア向けのバージョン (e.g. 2026 年 10 月 21 日の公開日時が最も新しい `v5.5.87-neoforge`)を同じプロジェクトで公開するプラグインがあるからである (srch-2)。この日の LuckPerms について、mism は、仕様書の srch-2 の例のとおり `v5.5.71-bukkit` を選ぶ。
- mism が `--limit` の値を通信の前に確かめるのは、形の合わない値で通信を始めないためである。
- 仕様書の dsrch-2 がデータパックの記述に `loaders` を含めないのは、データパックの記述に使う Modrinth のバージョンの `loaders` が必ず `datapack` を含み、利用者がデータパックを選ぶ手がかりにならないからである。
- 2026 年 10 月 21 日の True Ending の公開日時が最も新しい Modrinth のバージョンは、`loaders` が `fabric,forge,neoforge,quilt` の `1.1.4d` であり、`loaders` が `datapack` の `1.1.4d` は、その 26 秒前に公開された。この日の True Ending について、mism は、仕様書の dsrch-2 の例のとおり、`loaders` が `datapack` の `1.1.4d` を選ぶ。筆者は、この 2 つの公開日時を 2026 年 10 月 21 日に確認した。

## mism plugin versions と mism datapack versions の手順

### 責務

`subcommands/plugin/versions.rs` は `mism plugin versions` の手順の全体を、`subcommands/datapack/versions.rs` は `mism datapack versions` の手順の全体を担う。`subcommands/modrinth_versions.rs` は、2 つのモジュールが使う、この節の手順 3 から手順 6 を持つ。
この 2 つのモジュールは、仕様書の vers-1 から vers-6 と dvers-1 から dvers-6 を満たす。

### 方法

1. mism は、引数を `fetch_source.rs` で確かめる (vers-4、dvers-4)。
2. mism は、対象のサーバーを決める (plg-2、dp-2)。
3. mism は、プロジェクトのエンドポイントを呼び、識別子を確かめる (plg-3、dp-4)。
4. mism は、バージョンの一覧のエンドポイントを呼び、バージョンの列挙に使う Modrinth のバージョン (プラグインではすべて、データパックでは `loaders` が `datapack` を含むもの。「Modrinth の API」の節)だけを残す (vers-1、dvers-1)。
5. mism は、残した Modrinth のバージョンの必須の依存先の識別子を決める (vers-2、dvers-2、「Modrinth の API」の節)。
6. mism は、残した Modrinth のバージョンを `date_published` の新しい順に並べる。mism は、公開日時が同じものを、バージョンの一覧のエンドポイントが返した順のままにする (vers-1、dvers-1)。
7. mism は、値の名前の記述を、`output.rs` を通して書く (vers-6、dvers-6、plg-9、dp-12、out-1)。プラグインの値の名前は `version`、`release_type`、`software`、`newest_minecraft_version`、`required_dependencies` であり、データパックの値の名前は、そこから `software` を除いた 4 つである。対象のサーバーがあれば、mism は、その後に `declared` を足す。
8. mism は、並べた順に、`version_number`、`version_type`、`loaders` (プラグインだけ)、`game_versions` のうち最も新しいもの (「Minecraft のバージョンの新旧の判定」の節)、必須の依存先の識別子の一覧を並べ、対象のサーバーがあればその後に照らし合わせた結果 (vers-3、dvers-3)を足した記述を、`output.rs` を通して 1 つずつ改行で区切って書き、終了ステータス 0 で終了する (vers-1、vers-2、vers-3、dvers-1、dvers-2、dvers-3、plg-9、dp-12、out-1)。

通信のどれかが失敗したとき、mism は、手順 7 に進まないので標準出力に値の名前の記述も Modrinth のバージョンの記述も書かず、バージョンの列挙を中止してエラーを報告する (vers-5、dvers-5)。

### 判断

- 値の名前のうち `version`、`release_type`、`software` は、「Modrinth のファイルの選択」の節が問いとエラーで使う属性の名前と同じである。筆者がこの値の名前を `minecraft_version` にせず `newest_minecraft_version` にするのは、その値が対応する Minecraft のバージョンの一覧ではなく、その中で最も新しいもの 1 つだからである。`declared` は、検証の判定 (insp-3、dinsp-3)が同じ照らし合わせの結果に付けている名前と同じである。
- 仕様書の dvers-2 がデータパックの記述に `loaders` を含めないのは、仕様書の dsrch-2 と同じく、データパックの Modrinth のバージョンの `loaders` が必ず `datapack` を含むからである。

## mism plugin fetch と mism datapack fetch と取得の手順

### 責務

`subcommands/plugin/fetch.rs` は `mism plugin fetch` の手順の全体と、`mism plugin inspect` と `mism plugin install` が使う取得の手順を担い、`subcommands/datapack/fetch.rs` は `mism datapack fetch` の手順の全体と、`mism datapack inspect` と `mism datapack install` が使う取得の手順を担う。`subcommands/modrinth_fetch.rs` は、2 つのモジュールが使う、取得の手順の手順 2 のうち、ファイルの選択と、キャッシュのディレクトリに記録する値の決定を持つ。ファイルの保存 (手順 6)は、fetch-4 と dfetch-4 の文字列がプラグインとデータパックで違うので、2 つのモジュールがそれぞれ `fetched_file_cache.rs` を呼んで行う。
この 2 つのモジュールは、仕様書の fetch-1 から fetch-5、plg-5、plg-6、dfetch-1 から dfetch-5、dp-8、dp-9 を満たす。

### 方法

取得の手順は、次のとおりである。

1. mism は、<取得元の指定> を `fetch_source.rs` で解析する (plg-3、dp-4)。
2. 取得元が Modrinth のとき、mism は、プロジェクトのエンドポイントを呼んで識別子を確かめ (plg-3、dp-4)、`title` をプラグイン (またはデータパック)の名前とし (plg-5、dp-8)、バージョンの一覧のエンドポイントを呼び、`modrinth_api/selection.rs` で、プラグイン (またはデータパック)のファイルを 1 つに絞り (plg-10、plg-11、plg-12、inst-2、dp-5、dp-6、dinst-2)、そのファイルが属する Modrinth のバージョンの必須の依存先の識別子を決める。ファイルの名前は、そのファイルの `filename` である。
3. 取得元が URL のとき、mism は、URL からプラグイン (またはデータパック)の名前を作り、プラグインでは `mism: plugin name from the URL: <作った名前>`、データパックでは `mism: datapack name from the URL: <作った名前>` という文字列を書く (fetch-2、dfetch-2)。利用者が `--yes` を付けていなければ、mism は、プラグインでは `mism: plugin name (empty for <作った名前>): `、データパックでは `mism: datapack name (empty for <作った名前>): ` という問いを書き、標準入力から改行までの回答を 1 つ読む (fetch-2、dfetch-2)。標準入力が端末でないときも、mism は、標準入力から回答を読む。回答が空でなければ、mism は、回答をプラグイン (またはデータパック)の名前とし、空 (標準入力の終わりを含む)なら、作った名前をその名前とする (fetch-2、dfetch-2)。ファイルの名前は、その名前にファイルの名前の末尾 (プラグインでは `.jar`、データパックでは `.zip`)を付けたものである (fetch-2、dfetch-2)。
4. 取得元が手元のファイルのとき、プラグイン (またはデータパック)の名前とファイルの名前は、「取得元の指定」の節のとおりである (plg-5、dp-8)。
5. mism は、プラグイン (またはデータパック)の名前とファイルの名前を、`file_name.rs` で検査する (plg-6、dp-9)。
6. mism は、ファイルを、`fetched_file_cache.rs` の手順で `<キャッシュのディレクトリ>/<plugins|datapacks>/` の下に保存する (fetch-1 から fetch-4、dfetch-1 から dfetch-4)。

`fetch` は、取得の手順の後に、プラグイン (またはデータパック)の名前、バージョンの番号 (取得元が Modrinth でなければ `-`)、取得元の指定 (取得元が Modrinth なら plg-12 (または dp-6)の形、それ以外なら利用者が与えた文字列)、ファイルの SHA-256、キャッシュのディレクトリの中のファイルの絶対パスを並べた 1 つの記述を、`output.rs` を通して書き、終了ステータス 0 で終了する (fetch-5、dfetch-5、plg-9、dp-12、out-1)。fetch-4 (または dfetch-4)の場合、その名前とファイルのパスは、既にある方のものである (fetch-4、dfetch-4)。

### 性質

- d-fetch-1. 取得のダウンロードの途中で mism が `SIGINT` を受けたとき、mism は、終了ステータス 130 で終了し、キャッシュのディレクトリの中に SHA-256 のディレクトリを足さない。

### 判断

- mism が URL からの取得でダウンロードの前に名前を尋ねるのは、利用者が回答した後にダウンロードを待てるようにするためである。fetch-4 (または dfetch-4)で既にあるファイルと同じと分かったとき、mism は、回答された名前を使わない (fetch-4、dfetch-4)。
- mism が、取得元の指定のファイルが既にキャッシュのディレクトリにあっても `fetch` でダウンロードするのは、fetch-1 から fetch-3 と dfetch-1 から dfetch-3 が、取得のたびにダウンロードまたは複製すると定めているからである。ダウンロードしたファイルが既にあるものと同じなら、mism は、fetch-4 (または dfetch-4)のとおり新しいファイルを保存しない。
- mism が `fetch` でデータパックのファイルの `pack.mcmeta` を確かめないのは、仕様書の dfetch-1 から dfetch-5 が、プラグインの fetch-1 から fetch-5 と同じく、取得で中身を確かめると定めていないからである。mism は、検証 (dinsp-4)で確かめる。

## mism plugin inspect と mism datapack inspect と検証の手順

### 責務

`subcommands/plugin/inspect.rs` は `mism plugin inspect` の手順の全体と、`mism plugin place` と `mism plugin install` が使う検証の手順を担い、`subcommands/datapack/inspect.rs` は `mism datapack inspect` の手順の全体と、`mism datapack place` と `mism datapack install` が使う検証の手順を担う。
この 2 つのモジュールは、仕様書の insp-1 から insp-4 と dinsp-1 から dinsp-5 を満たす。

### 方法

1. mism は、対象のサーバーを決める (plg-2、dp-2)。
2. mism は、<取得元の指定> を解析し (plg-3、dp-4)、キャッシュのディレクトリからその取得元の指定のファイルを探す (insp-1、dinsp-1、「キャッシュのディレクトリのプラグインとデータパック」の節)。
3. 見つからなければ、mism は、取得の手順を行う (insp-1、dinsp-1)。
4. mism は、ファイルの宣言を読む (insp-1、dinsp-1)。プラグインでは、mism は、`plugin_jar.rs` で jar ファイルの宣言を読み、宣言を持たなければ、処理を中止してエラーを報告する (insp-4)。データパックでは、mism は、`datapack_zip.rs` で `pack.mcmeta` を読み、dinsp-4 の条件が成り立てば、処理を中止してエラーを報告する (dinsp-4)。対象のサーバーがあれば、mism は、この手順で `server_directory/server_jar.rs` で対象のサーバーのデータパックの形式の番号も読み、読めなければ、処理を中止してエラーを報告する (dinsp-5)。
5. mism は、プラグインでは `plugin_compatibility.rs` で、データパックでは `datapack_compatibility.rs` で検証の記述の値 (プラグインでは 13 個、データパックでは 9 個)を作り、その記述を改行で区切って書く (insp-2、dinsp-2)。
6. 対象のサーバーがあれば、mism は、検証の判定を作り、出力する条件が成り立つ判定の記述を書く (insp-3、dinsp-3)。データパックでは、mism は、その後に dinsp-6 の警告を書く (dinsp-6)。プラグインでは、mism は、`server_directory/plugins.rs` で `plugins/` の直下の jar ファイルの宣言を読み、`mism.toml` の記録を読む。データパックでは、mism は、手順 4 で読んだ対象のサーバーのデータパックの形式の番号と、`mism.toml` の記録を使う。
7. mism は、終了ステータス 0 で終了する (insp-1、dinsp-1)。

手順 4 から手順 6 が、`place` と `install` が使う検証の手順である。

### 性質

- d-inspect-1. キャッシュのディレクトリにその取得元の指定のファイルがあるとき、`inspect` は、通信しない。

## mism plugin place と mism datapack place と配置の手順

### 責務

`subcommands/plugin/place.rs` は `mism plugin place` の手順の全体と、`mism plugin install` が使う配置の手順を担い、`subcommands/datapack/place.rs` は `mism datapack place` の手順の全体と、`mism datapack install` が使う配置の手順を担う。
この 2 つのモジュールは、仕様書の place-1 から place-6 と dplace-1 から dplace-6 を満たす。

### 方法

1. mism は、対象のサーバーを決める (plg-1、dp-1)。データパックでは、mism は、置くディレクトリ (ワールドのディレクトリの `datapacks/`)も決める (dp-3、「プラグインとデータパックを置くディレクトリ」の節)。
2. mism は、<取得元の指定> を解析し (plg-3、dp-4)、キャッシュのディレクトリからその取得元の指定のファイルを探す (place-1、dplace-1)。見つからなければ、mism は、処理を中止してエラーを報告する (place-1、dplace-1)。mism は、この手順でも以降の手順でも、通信しない (place-1、dplace-1)。
3. mism は、検証の手順を行う (place-6、dplace-6)。プラグインの宣言が無ければ、mism は、insp-4 のとおりエラーを報告する (place-6)。プラグインの検証の判定に `incompatible` か `conflict` が 1 つでもあれば、mism は、検証の記述を書いた後に処理を中止し、そのどちらがあったかを伝えるエラーを報告する (place-6)。dinsp-4 か dinsp-5 の条件が成り立てば、mism は、検証の記述を書かずにエラーを報告する (dplace-6)。データパックの検証の判定と dinsp-6 の警告は、配置を止めない (dplace-6、「データパックの検証の値と判定」の節)。
4. mism は、place-3 と place-4 (または dplace-3 と dplace-4)の条件を確かめる (「プラグインとデータパックを置くディレクトリ」の節)。
5. mism は、中断のフラグを確認する。
6. mism は、ファイルを `server_directory/placed_files.rs` の手順で配置する (place-2、dplace-2、plg-8、dp-11)。
7. mism は、名前、バージョンの番号 (取得元が Modrinth でなければ `-`)、ファイルの名前を並べた記述と、plg-7 (または dp-10)の文字列を、この順に書き、終了ステータス 0 で終了する (place-5、dplace-5、plg-7、dp-10、plg-9、dp-12)。

手順 5 から手順 7 が、`install` が使う配置の手順である。

## mism plugin install と mism datapack install の手順

### 責務

`subcommands/plugin/install.rs` は `mism plugin install` の手順の全体を、`subcommands/datapack/install.rs` は `mism datapack install` の手順の全体を担う。
この 2 つのモジュールは、仕様書の inst-1 から inst-7 と dinst-1 から dinst-7 を満たす。

### 方法

1. mism は、対象のサーバーを決める (plg-1、dp-1)。データパックでは、mism は、データパックを置くディレクトリ (ワールドのディレクトリの `datapacks/`)も決める (dp-3)。
2. 利用者が `--yes` を付けておらず、標準入力が端末でなければ、mism は、通信せずに処理を中止し、`--yes` を付けるよう案内するエラーを報告する (inst-7、dinst-7)。
3. mism は、<取得元の指定> を解析する (plg-3、dp-4)。`modrinth:` の取得元の指定が属性の値を 1 つも持たないときだけ、mism は `--lazy` を使い、それ以外のとき (URL、手元のファイル、値を持つ `modrinth:` の取得元の指定)、mism は `--lazy` を無視する (inst-2、inst-3、dinst-2、dinst-3)。
4. mism は、取得の手順を行う (inst-1、inst-2、inst-4、dinst-1、dinst-2、dinst-4)。
5. mism は、検証の手順を行う (inst-4、dinst-4)。プラグインの宣言が無いとき、または dinsp-4 か dinsp-5 の条件が成り立つとき、mism は、検証の記述を書かずに処理を中止してエラーを報告する (insp-4、dinsp-4、dinsp-5)。プラグインの検証の判定に `incompatible` か `conflict` が 1 つでもあれば、mism は、検証の記述を書いた後に処理を中止してエラーを報告する (inst-4)。データパックの検証の判定と dinsp-6 の警告は、インストールを止めない (dinst-4)。
6. mism は、place-3 と place-4 (または dplace-3 と dplace-4)の条件を確かめる (inst-4、dinst-4)。
7. 利用者が `--yes` を付けていれば、mism は、尋ねずに手順 8 に進む (inst-6、dinst-6)。付けていなければ、mism は、プラグインでは `mism: place <ファイルの名前> in plugins/? (y/n): `、データパックでは `mism: place <ファイルの名前> in <ワールドのディレクトリ>/datapacks/? (y/n): ` という問いを書き、標準入力から改行までの回答を 1 つ読む (inst-5、dinst-5)。回答が `y` か `yes` であれば、mism は、手順 8 に進む (inst-5、dinst-5)。それ以外であれば、mism は、`mism: not placing <ファイルの名前>` という文字列を書き、置くディレクトリと `mism.toml` を変えずに終了ステータス 0 で終了する (inst-5、dinst-5)。
8. mism は、配置の手順を行う (inst-4、dinst-4、place-2、place-5、dplace-2、dplace-5、plg-7、dp-10)。

### 性質

- d-install-1. 標準入力が端末でなく、利用者が `--yes` を付けていないとき、`install` は、Modrinth にも URL にもリクエストを送らずにエラーを報告し、キャッシュのディレクトリを変えない。

### 判断

- mism が inst-7 と dinst-7 の条件を通信の前に確かめるのは、条件が通信の前に分かり、inst-5 (または dinst-5)の問いの段階で必ず失敗するインストールのためにファイルをダウンロードしないためである。
- mism が place-3 と place-4 (または dplace-3 と dplace-4)の条件を問いの前に確かめるのは、利用者が `y` と答えた後になって、mism がそのファイルを配置できないことを利用者に伝える、ということが起きないようにするためである。
- データパックの問いがワールドのディレクトリの絶対パスを含むのは、`level-name` の値によって置くディレクトリが `<対象のサーバーのディレクトリ>/world/datapacks/` 以外になり得るので、利用者が置くディレクトリを問いの中で確かめられるようにするためである。

## mism plugin list と mism datapack list の手順

### 責務

`subcommands/plugin/list.rs` は `mism plugin list` の手順の全体を、`subcommands/datapack/list.rs` は `mism datapack list` の手順の全体を担う。
この 2 つのモジュールは、仕様書の list-1 から list-4 と dlist-1 から dlist-4 を満たす。

### 方法

利用者が `--global` を付けていないとき、mism は、対象のサーバーと置くディレクトリを決め (plg-1、dp-1、dp-3)、`server_directory/placed_files.rs` で、プラグイン (またはデータパック)の記録と置くディレクトリの直下のファイルを突き合わせ、その記述を 1 つずつ改行で区切って書き、終了ステータス 0 で終了する (list-1、list-2、dlist-1、dlist-2、plg-9、dp-12)。

利用者が `--global` を付けたとき、mism は、`fetched_file_cache.rs` でキャッシュのディレクトリにあるプラグイン (またはデータパック)の一覧を作り、名前の昇順に、名前が同じものは SHA-256 の昇順に並べる。mism は、各ファイルについて、`source.toml` の `name`、`source` から取り出したバージョンの番号 (無ければ `-`)、キャッシュのディレクトリの中の SHA-256 のディレクトリの名前 (ファイルの SHA-256)、`source.toml` の `source` を並べた記述を 1 つずつ改行で区切って書き、終了ステータス 0 で終了する (list-3、list-4、dlist-3、dlist-4、plg-9、dp-12)。fetch-4 (または dfetch-4)で `source.toml` を書き換えたファイルについて、この記述は、書き換えた後の取得元の指定とバージョンの番号を含む (list-4、dlist-4)。

### 性質

- d-list-1. `list --global` の出力は、名前の昇順であり、名前が同じものはファイルの SHA-256 の昇順である。

## mism plugin remove と mism datapack remove の手順

### 責務

`subcommands/plugin/remove.rs` は `mism plugin remove` の手順の全体を、`subcommands/datapack/remove.rs` は `mism datapack remove` の手順の全体を担う。
この 2 つのモジュールは、仕様書の rm-1 から rm-3 と drm-1 から drm-3 を満たす。

### 方法

1. mism は、対象のサーバーと置くディレクトリを決める (plg-1、dp-1、dp-3)。
2. mism は、中断のフラグを確認する。
3. mism は、`server_directory/placed_files.rs` の手順で、プラグイン (またはデータパック)の記録とファイルを削除する (rm-1、rm-2、rm-3、drm-1、drm-2、drm-3、plg-8、dp-11)。
4. `mism.toml` の記録の `file_name` が指すファイルを、置くディレクトリから削除したとき、mism は、plg-7 (または dp-10)の文字列を書き、終了ステータス 0 で終了する (rm-1、drm-1、plg-7、dp-10)。記録のファイルが置くディレクトリに無かったとき、mism は、`mism: <置くディレクトリ>/<ファイルの名前> does not exist, so only the record of <名前> was removed` という文字列を書き、終了ステータス 0 で終了する (rm-3、drm-3)。`<置くディレクトリ>` は、プラグインでは `plugins` であり、データパックではワールドのディレクトリの `datapacks` の絶対パスである。

## テスト

### 責務

テストは、仕様書のすべての番号付きの文と、設計書のすべての「性質」の見出しの下の番号付きの文 (e.g. `d-cache-1`)が満たされていることを、`cargo test` 1 回で確かめる。
規則は `docs/development-rules.md` の「テストの規則」にあり、この節はその規則を mism でどう実現するかを定める。

### 方法

変更を行う者は、`tests/` のファイルを、仕様書の節と同じ分け方 (`command_line.rs`、`output.rs`、`new_server.rs`、`versions.rs`、`cache.rs`)で作る。仕様書の「プラグインの管理」の節については、変更を行う者は、その下の節ごとに `plugin_common.rs` (対象のサーバーと取得元の指定、サブコマンドに共通する規則)、`plugin_search.rs`、`plugin_versions.rs`、`plugin_fetch.rs`、`plugin_inspect.rs`、`plugin_place.rs`、`plugin_install.rs`、`plugin_list.rs`、`plugin_remove.rs` を作る。仕様書の「データパックの管理」の節については、変更を行う者は、その下の節ごとに `datapack_common.rs` (対象のサーバーと取得元の指定、サブコマンドに共通する規則)、`datapack_search.rs`、`datapack_versions.rs`、`datapack_fetch.rs`、`datapack_inspect.rs`、`datapack_place.rs`、`datapack_install.rs`、`datapack_list.rs`、`datapack_remove.rs` を作る。
テストを書く者は、各テスト関数の `///` コメントに、そのテスト関数が確かめる仕様書の番号付きの文の番号か、設計書の「性質」の番号付きの文の番号を書く。

偽の PaperMC は、テストの中で立てるローカルの HTTP サーバー (wiremock)である。
偽の PaperMC は、本物の PaperMC のバージョンの一覧のエンドポイントのレスポンスとして保存したものと、偽の jar ファイル (中身は任意のバイト列で、テストがその SHA-256 を JSON に入れる)を返す。
本物の PaperMC のレスポンスとして保存したものは、`tests/fixtures/` にある。筆者は、Paper と Folia のバージョンの一覧と、Paper `26.3` の `206` と Folia `26.3` の `3` のダウンロードの情報を、2026 年 10 月 20 日に保存した。
wiremock は tokio の上で動くので、各テストは、複数のスレッドを持つ tokio のランタイムを作り、その上で偽の PaperMC を立てる。テストのスレッドが mism のプロセスの終了を待って止まっている間も、偽の PaperMC は、ランタイムの別のスレッドで応答する。
wiremock は、HTTP のレスポンスの本文を少しずつ送ることができないので、ダウンロードが遅くても進んでいる場合 (d-http-2)のテストは、テストの中のスレッドで動く小さな TCP サーバーに偽の jar ファイルを返させ、その TCP サーバーに HTTP のレスポンスの本文を間隔を空けて送らせる。
テストは、環境変数 `MISM_PAPERMC_URL` で偽の PaperMC の URL を mism に渡す。

偽の `java` は、シェルスクリプトである。
偽の `java` は、`-version` には決まった出力を返し、`-jar` では `server.properties` と `eula=false` の `eula.txt` を書いて終了する。
テストは、失敗用 (何も書かない)、中断用 (`sleep` する)、古い Java 用 (`-version` が 21 を返す)の 3 種類の別のスクリプトも用意する。
偽の `java` は、起動された回数と引数をファイルに記録し、テストがそれを読む。
テストは、`PATH` の先頭にこのスクリプトを置いたディレクトリを足す。

各テストは、一時ディレクトリを作り、`HOME`、`XDG_CACHE_HOME`、`PATH`、`MISM_PAPERMC_URL`、`MISM_DOWNLOAD_IDLE_TIMEOUT` を設定して mism の実行ファイルをプロセスとして起動し、標準出力、標準エラー出力、終了ステータス、mism が作ったサーバーのディレクトリと一時ディレクトリの中身を確かめる。
テストは、ファイルが現れることやプロセスが終わることを待つとき、期限を決めて、その状態になるまで繰り返し確認する。
標準出力が端末であるときの色 (out-3)を確かめるテストは、nix の `openpty` で擬似端末を作り、その端末として振る舞う側 (slave)を mism の標準出力につないで mism を起動し、制御する側 (master)から読んだ内容にエスケープシーケンスがあることを確かめる。

偽の Modrinth は、偽の PaperMC と同じく、テストの中で立てるローカルの HTTP サーバー (wiremock)であり、パスが `/v2` で始まるリクエストに応える。テストは、環境変数 `MISM_MODRINTH_URL` に `<偽の Modrinth の URL>/v2` を入れて mism に渡す。
偽の Modrinth は、`tests/fixtures/` に保存した本物の Modrinth のレスポンスを返す。変更を行う者は、次のリクエストに対する本物の Modrinth のレスポンスを、次の名前で保存する。筆者は、これらのレスポンスを 2026 年 10 月 21 日に保存した。

- `modrinth-search-multiverse-portals.json` は、`GET /v2/search?query=multiverse portals&facets=[["project_type:plugin"]]&limit=10` (変更を行う者は、クエリ文字列の値をパーセントエンコーディングする)のレスポンスである。2026 年 10 月 21 日には、このレスポンスは `multiverse-portals`、`multiverse-netherportals`、`multiverse-signportals`、`locationlast`、`playerportals` の 5 つのプロジェクトを、この順に返した。
- `modrinth-search-no-hit.json` は、`query` を `zzqqxxnotaplugin` にした同じリクエストのレスポンスであり、2026 年 10 月 21 日には `hits` が空であった。
- `modrinth-project-<slug>.json` は、`GET /v2/project/<slug>` のレスポンスであり、変更を行う者は、`luckperms`、`fastasyncworldedit`、`multiverse-portals` の 3 つを保存する。
- `modrinth-versions-<slug>.json` は、`GET /v2/project/<プロジェクトの id>/version?include_changelog=false` のレスポンスであり、変更を行う者は、`luckperms`、`fastasyncworldedit`、`multiverse-portals`、`multiverse-netherportals`、`multiverse-signportals`、`locationlast`、`playerportals` の 7 つを保存する。`luckperms` は、すべての Modrinth のバージョンが 1 つのファイルを持ち、バージョンの番号が重ならない例である。`fastasyncworldedit` は、同じバージョンの番号を持つ 2 つの Modrinth のバージョン (`2.16.1`)と、2 つのファイルを持つ Modrinth のバージョン (`2.14.1`)の例である。`multiverse-portals` は、必須の依存先 (`project_id` が `3wmN97b8` の Multiverse-Core)を持つ例である。
- `modrinth-projects-multiverse-core.json` は、`GET /v2/projects?ids=["3wmN97b8"]` のレスポンスである。

変更を行う者は、データパックのテストのために、次のリクエストに対する本物の Modrinth のレスポンスを、次の名前で保存する。筆者は、2026 年 10 月 21 日に、これらのリクエストに対する本物の Modrinth のレスポンスが次に書く内容であることを確認した。

- `modrinth-search-datapack-true-ending.json` は、`GET /v2/search?query=true ending&facets=[["project_type:datapack"]]&limit=10` のレスポンスである。2026 年 10 月 21 日には、このレスポンスは `true-ending`、`stellarity_x_true_ending_patch`、`vanilla-true-ending` の 3 つのプロジェクトを、この順に返した。
- `modrinth-search-datapack-no-hit.json` は、`query` を `zzqqxxnotaplugin` にした同じリクエストのレスポンスであり、2026 年 10 月 21 日には `hits` が空であった。
- 変更を行う者は、`GET /v2/project/<slug>` のレスポンス (`modrinth-project-<slug>.json`)を、`true-ending`、`stellarity_x_true_ending_patch`、`navas-zamega` の 3 つについて保存する。
- 変更を行う者は、`GET /v2/project/<プロジェクトの id>/version?include_changelog=false` のレスポンス (`modrinth-versions-<slug>.json`)を、`true-ending`、`stellarity_x_true_ending_patch`、`vanilla-true-ending`、`navas-zamega` の 4 つについて保存する。`true-ending` は、同じ番号 (`1.1.4`)の Modrinth のバージョンが `loaders` が `datapack` のものとモッドのものに分かれる例であり、`loaders` が `datapack` の 2 つの Modrinth のバージョンが同じ番号 (`1.1.4b`)を持ち、対応する Minecraft のバージョンの一覧で区別される例である。`stellarity_x_true_ending_patch` は、2 つの必須の依存先 (`true-ending` と `stellarity`)を持ち、同じ番号 (`1.0.0`)の 2 つの Modrinth のバージョンがファイルの名前でだけ区別される例である。`navas-zamega` は、`file_type` が `optional-resource-pack` のファイルを持つ Modrinth のバージョン (`1.2.1`)の例である。
- `modrinth-projects-stellarity-true-ending.json` は、`GET /v2/projects?ids=["MCnBYP0b","bZgeDzN8"]` のレスポンスである。

偽の Modrinth は、プロジェクトの id を、保存したプロジェクトのレスポンスと検索のレスポンスから得て、バージョンの一覧のエンドポイントのパスに使う。偽の Modrinth は、検索のエンドポイントへのリクエストに、クエリ文字列の `query` と `facets` の値に合う保存したレスポンスを返す。
偽の Modrinth は、テストがダウンロードさせるファイルについて、保存したバージョンの一覧の `files` の要素の `url` を偽の Modrinth の上の URL に、`hashes.sha512` をテストが用意した jar ファイル (またはデータパックのファイル)の SHA-512 に書き換えてから返し、その URL でテストが用意したファイルを返す。この書き換えは、偽の PaperMC が偽の jar ファイルの SHA-256 を JSON に入れるのと同じ考え方による。
テストが URL から取得させるとき (fetch-2)は、偽の Modrinth と同じ wiremock の上の任意のパス (e.g. `/downloads/Example.jar`)で jar ファイルを返す。
テストが用意する jar ファイルは、テストが zip のクレートで作る小さな zip 形式のファイルであり、テストが書いた `plugin.yml` か `paper-plugin.yml` (または両方、またはどちらも無いもの)を持つ。宣言の内容は、各テストが確かめる条件に合わせて、テストが文字列として書く。
テストが用意するデータパックのファイルは、テストが zip のクレートで作る小さな zip 形式のファイル (最上位に `pack.mcmeta` を持つものと、ディレクトリの中にだけ持つもの)と、zip 形式でないファイルである。`pack.mcmeta` の内容は、各テストが確かめる条件に合わせて、テストが文字列として書く。
データパックの管理のテストは、テストが用意するサーバーのディレクトリに、テストが決めた `level-name` の値を書いた `server.properties` を置く。対象のサーバーのデータパックの形式の番号を使うテスト (dinsp-3 の `pack_format`)は、テストが zip のクレートで作る `versions/<Minecraft のバージョン>/paper-<Minecraft のバージョン>.jar` を置き、その最上位に、Paper 26.3 の `version.json` と同じ形の `pack_version` を持つ `version.json` を書く。

変更を行う者は、偽の Modrinth、テストが用意する jar ファイルとデータパックのファイルの作成、テストが用意するサーバーのディレクトリ、mism の標準入力の接続を、`tests/common/` の新しいファイル (e.g. `fake_modrinth.rs`、`test_plugin_jar.rs`、`test_datapack_zip.rs`、`plugin_server_directory.rs`、`datapack_server_directory.rs`、プラグインの管理のテストとデータパックの管理のテストが使うテスト用の環境の `management_environment.rs`)に置き、`tests/common/mod.rs` を大きくしない。
プラグインの管理とデータパックの管理のテストは、一時ディレクトリの中に、`software`、`minecraft_version`、`software_version` (と、必要なら `[[plugins]]` と `[[datapacks]]` の記録)を書いた `mism.toml` を持つディレクトリを作り、そのディレクトリをカレントディレクトリにして mism を起動する。テストは、`mism new` を実行してサーバーのディレクトリを作らない。
テストは、キャッシュのディレクトリの中身を手で作らず、`mism plugin fetch` (データパックのテストでは `mism datapack fetch`)を実行して用意する。テストがキャッシュのディレクトリの中身を自分で作ると、テストが、キャッシュのディレクトリの中のディレクトリの階層とファイルの名前の付け方 (「キャッシュのディレクトリのプラグインとデータパック」の節)に依存するからである。その階層と名前の付け方そのものを確かめるテスト (d-pcache-1、d-pcache-3)と、壊れたキャッシュのディレクトリの扱いを確かめるテスト (d-pdir-1)は、例外として、`fetch` が作ったものを書き換える。
テストは、mism の標準入力に、次の 3 つのどれかをつなぐ。1 つ目は、テストが決めたバイト列を書いた後に閉じるパイプであり、標準入力が端末でない場合のテスト (fetch-2、dfetch-2、plg-11、inst-7、dinst-7)に使う。2 つ目は、`openpty` で作った擬似端末の、端末として振る舞う側 (slave)であり、標準入力が端末である場合のテスト (plg-11、inst-5、dinst-5)に使う。テストは、回答を擬似端末の制御する側 (master)に先に書いておく。擬似端末のカノニカルモード (改行までの入力をまとめて読み手に渡す端末の動作)が、mism の 1 回の読み取りに改行までの回答を 1 つずつ渡すので、テストは、mism が問いを書くのを待たなくてよい。3 つ目は、何も届かず閉じないパイプであり、既存のテストが使う。
d-prompt-1 のテストは、擬似端末の端末として振る舞う側 (slave)を標準入力につなぎ、標準出力を別のスレッドで読み続けて、問いが現れるまで期限を決めて待ち、`SIGINT` を送る。d-modrinth-1 のテストは、偽の Modrinth のレスポンスを遅らせ、最初のリクエストが届いた後に `SIGINT` を送り、偽の Modrinth が受け取ったリクエストの数を確かめる。
plg-8 のテストは、配置の途中の失敗をサーバーのディレクトリへの書き込みの権限を外して起こし (`mism.toml` の置き換えが失敗する)、削除の途中の失敗を `plugins/` への書き込みの権限を外して起こす。この 2 つのテストは、root ではない利用者で `cargo test` を実行することを前提とする。
dp-11 の配置の途中の失敗のテストと d-dpdir-2 のテストは、`level-name` の値を一時ディレクトリの中の別のディレクトリの絶対パス (d-dpdir-2 では `<一時ディレクトリ>/outside/worlds/survival`)にし、対象のサーバーのディレクトリへの書き込みの権限を外して、`mism.toml` の置き換えを失敗させる。ワールドのディレクトリは書き込める場所にあるので、mism は、ワールドのディレクトリと `datapacks/` を作った後に失敗する。dp-11 の削除の途中の失敗のテストは、`datapacks/` への書き込みの権限を外して失敗を起こす。これらのテストも、root ではない利用者で `cargo test` を実行することを前提とする。

変更を行う者は、通信もファイルも使わない処理 (Minecraft のバージョンの新旧の判定、バージョンの選択、`mism.toml` の読み書き、`eula.txt` の書き換え、取得元の指定の解析、ファイルの名前の検査、Modrinth のファイルの選択、検証の値と判定、必須の依存先の識別子の解決、`server.properties` の読み取り、データパックの形式の番号の比較)を、そのモジュールのファイルの中の単体テストで確かめる。
バージョンの選択の単体テストでは、テストが、決まった値を返す偽の実装を API のトレイトに与える。Modrinth のファイルの選択と必須の依存先の識別子の解決の単体テストでは、テストが、決まった値を返す偽の実装を、Modrinth の API のトレイトと尋ねる処理のトレイトに与える。本物のレスポンスに例の無い条件 (d-modrinth-2、d-select-2 から d-select-4)は、これらの単体テストで確かめる。
`plugin_jar.rs`、`datapack_zip.rs`、`server_directory/server_jar.rs` の単体テストは、テストの中で zip 形式のバイト列を作り、それを読む。

本物の PaperMC に対するテストは、環境変数 `MISM_TEST_REAL_API=1` のときだけ動く 1 件 (`mism versions paper` が成功し、少なくとも 1 つの Minecraft のバージョンを標準出力に出力すること)である。
本物の Modrinth に対するテストは、環境変数 `MISM_TEST_REAL_API=1` のときだけ動く 1 件 (`mism plugin search luckperms` が成功し、標準出力が `mism: modrinth:luckperms` とタブ 1 文字で始まる記述を含むこと)である。

### 判断

- テストが偽の PaperMC と偽の Modrinth に本物のレスポンスから保存したものを返させるのは、テストを書く者が手で書いた JSON では、本物のレスポンスと JSON のキーや値の型が違ってもテストが通ってしまうからである。
- テストが `java` を `PATH` で差し替えるのは、mism が `java` を `PATH` から探すという方法をそのまま使えるので、テストのための環境変数を mism の実行ファイルに足さずに済むからである。
- テストがプラグインの jar ファイルを自分で作るのは、宣言の内容 (e.g. `folia-supported`、`api-version`、依存先)をテストの条件ごとに変える必要があるからである。jar ファイルの中身は Modrinth の API のレスポンスではないので、本物から保存したものを使うという規則の対象ではない。偽の PaperMC も、中身が任意のバイト列の偽の jar ファイルを返している。
- 検索のレスポンスに `multiverse portals` の語のものを使うのは、`luckperms` の語の検索が 10 件を返し、10 件すべてのバージョンの一覧を保存することになるからである。`multiverse portals` の語の検索は 5 件を返し、5 件のバージョンの一覧の合計は約 90 KB である。筆者は、この件数と大きさを 2026 年 10 月 21 日に確認した。

## 未決事項

- リポジトリの所有者は、mism が選んだ Minecraft のバージョンの `builds` が空の配列である場合の扱いを、仕様書に書いていない。筆者は、`builds` が空になり得るかも確認していない。
- 筆者は、`version.java.version.minimum` の意味を、PaperMC のドキュメントで確認していない。
- 筆者は、mism が `SIGTERM` の後に `SIGKILL` を送るまでの 10 秒が十分かを、確認していない。
- 筆者は、`renameat2` の `RENAME_NOREPLACE` が使えるカーネルのバージョンを、確認していない。
- wiremock は、2026 年 10 月 21 日の時点で、最後のリリースが 2025 年 8 月 24 日であり、「依存するクレートの追加の規則」の 3 番目 (直近 1 年以内のリリース)を満たさない。リポジトリの所有者は、wiremock を使い続けるか、「依存するクレートの追加の規則」の 5 番目に従って別の方法に替えるかを決めていない。
- serde-saphyr に依存している公開クレートの数は 420 であり、serde-saphyr は「依存するクレートの追加の規則」の 3 番目を満たさない (「依存するクレート」の節)。リポジトリの所有者は、YAML のクレートのどれかがその 3 番目を満たしたときに、選び直すかを決めていない。
- 筆者は、jar ファイルが `plugin.yml` と `paper-plugin.yml` の両方を持つとき、Paper がどちらを使うかを確認していない。mism は、`plugin.yml` を読む。
- 筆者は、Paper が引用符の無い `api-version: 1.20` を `1.20` と読むか `1.2` と読むかを確認していない。mism は、`1.20` と読む。
- 筆者は、Paper が依存先の名前とプラグインの名前の大文字と小文字を区別するかを確認していない。mism は、区別する。
- 筆者は、Paper が `plugins/` の中の名前が `.jar` で終わるファイルだけをプラグインとして読み込むことを確認していない。mism の作業用の名前と、手元の jar ファイルの名前の検査は、このことを前提にしている。
- 筆者は、Modrinth がプロジェクトのエンドポイントのパスの `%22` と `%60` を `"` と `` ` `` に戻して slug と比べるかを確認していない。
- 筆者は、`version_id` だけを持つ Modrinth の依存先を、実際のレスポンスでは見ていない。
- `mism plugin remove` と `mism datapack remove` は、`mism.toml` を書き換えた後に、作業用の名前に変えたファイルの削除が失敗すると、エラーを報告するが、`mism.toml` は既に変わっているので、その場合には plg-8 と dp-11 が成り立たない。筆者は、この場合を防ぐ方法を決めていない。
- 筆者は、`mism versions` が `SIGINT` または `SIGTERM` を受けたときの振る舞いを決めていない。mism がハンドラを登録するとシグナルの既定の動作 (終了)がハンドラの動作に置き換わるので、`mism versions` は、通信が終わるまで動き続けてから終了する。
- 筆者は、ダウンロードの情報のエンドポイントが返す jar ファイルの名前に `/` や `..` が含まれる場合の扱いを決めていない。今の mism は、その名前をそのままファイルの名前に使う。
- 筆者は、中断のときに作業用のディレクトリの削除が失敗した場合に、その失敗をどう伝えるかを決めていない。今の mism は、サーバーの作成が中断されたことだけを標準出力に出力する。
- 筆者は、ureq が既定で読むプロキシの環境変数 (`HTTP_PROXY`、`HTTPS_PROXY`、`ALL_PROXY`、`NO_PROXY`)を mism がどう扱うかを決めていない。今の mism は、ureq の既定のとおりにそれらを読む。
- 筆者は、Paper の起動のオプションや設定のファイルでワールドを置くディレクトリを変えられるかと、変えたときに Paper がどのディレクトリの `datapacks/` を読むかを、確認していない。mism は、`level-name` の値だけからワールドのディレクトリを決める。
- 筆者は、Minecraft のサーバーが `server.properties` を `java.util.Properties` の `load` で読むかと、UTF-8 として読めない `server.properties` をどう読むかを、確認していない。mism は、`java.util.Properties` の形式で読み、UTF-8 として読めなければエラーを報告する。
- 筆者は、`server.properties` が `level-name` を持たないときと、`level-name` の値が空のときに、サーバーがどのディレクトリをワールドのディレクトリにするかを確認していない。mism は、どちらの場合もエラーを報告する。
- 筆者は、Paper が `datapacks/` の中の、名前が `.zip` で終わらない zip 形式のファイルと、`datapacks/` の直下のディレクトリを、データパックとして読み込むかを確認していない。mism は、名前が `.zip` で終わるファイルだけを扱う (dp-8、dfetch-2、dlist-2)。
- データパックの Modrinth のバージョンが `file_type` が `required-resource-pack` のファイル (そのデータパックが必要とするリソースパック)を持つとき、mism は、そのファイルを置かず、そのファイルがあることを利用者に伝えない。仕様書は、データパックが必要とするリソースパックの扱いを定めていない。
- 筆者は、サーバーが動いている間に `datapacks/` に足したデータパックを、サーバーが `/reload` のコマンドで有効にするかを、確認していない。mism は、変更がサーバーの次の起動で反映される旨を出力する (dp-10)。
- 筆者は、`pack.mcmeta` が `min_format` と `max_format`、`supported_formats`、`pack_format` を同時に持つときに Minecraft のサーバーがどれを使うかと、サーバーが形式の番号の範囲を「データパックの検証の値と判定」の節の規則で比べることを、サーバーの実装では確認していない。mism は、Minecraft Wiki の pack.mcmeta の記事の規則で比べる。
- 筆者は、Minecraft 1.21.8 以前の Paper (または Folia)の `versions/` の jar ファイルの `version.json` を、実物では確認していない。mism は、`pack_version` の `data` を、Minecraft Wiki の version.json の記事の記述に従って読む。
- データパックの記録は、ファイルを置いたワールドのディレクトリを持たない。利用者が `level-name` を変えた後は、`mism datapack list` は前のワールドのディレクトリの記録のファイルを `missing` と出力し、`mism datapack remove` は記録だけを削除する。筆者は、ワールドのディレクトリごとに記録を分けるかを決めていない。
- 筆者は、Minecraft のサーバーが、無圧縮と DEFLATE 以外の圧縮の方式を使う zip 形式のデータパックを読み込むかを確認していない。mism は、そのようなファイルの `pack.mcmeta` を読めないので、エラーを報告する (「依存するクレート」の節の zip)。
