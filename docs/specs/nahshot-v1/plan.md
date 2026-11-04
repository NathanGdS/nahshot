# nahshot v1 — Plano de Implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** App de captura de tela para Windows, estilo Lightshot, que salva todo print numa galeria organizada (álbuns = pastas, etiquetas, favoritos), leve o bastante pra ficar parado na bandeja com menos de 30 MB.

**Architecture:** Workspace Cargo com três peças. `nahshot-core` é Rust puro: pasta, SQLite, álbuns, sync, miniaturas, 100% testável. `nahshot-capture` é o overlay nativo (winit + softbuffer + tiny-skia) numa thread própria e persistente, mais os hooks de teclado e o registro do Windows; nenhum webview no caminho da captura. `src-tauri` é o app Tauri v2: bandeja, configurações e comandos da galeria. Ele começa **sem nenhuma janela** e só cria WebView2 quando a galeria ou as configurações abrem, destruindo-o ao fechar. A UI é TypeScript puro + Vite, sem framework.

**Tech Stack:** Rust (edition 2021, stable), Tauri 2, winit 0.30, softbuffer, tiny-skia, fontdue, xcap, windows (crate), winreg, rusqlite (bundled), notify + notify-debouncer-mini, image, trash, arboard, chrono, thiserror, serde; UI: TypeScript, Vite, Vitest.

**Spec:** `docs/specs/nahshot-v1/design.md`

## Global Constraints

- Windows 10 e 11, 64-bit. Sem privilégios de administrador. 100% local, sem rede.
- Rust + Tauri v2 (decisão do usuário).
- Idle (na bandeja, galeria fechada): < ~30 MB somando o processo nahshot **e** processos filhos (WebView2). Parado não pode haver webview viva.
- Print Screen → overlay visível: < ~150 ms. Abrir galeria a frio: ~1 s aceitável.
- Pasta padrão dos prints: `%USERPROFILE%\Pictures\nahshot\`; prints sem álbum em `AAAA-MM\`; álbuns em `Álbuns\<nome>\`.
- Nome de arquivo: `AAAA-MM-DD_HH-MM-SS.png` (horário local).
- Registro: `HKCU\Control Panel\Keyboard`, valor DWORD `PrintScreenKeyForSnippingEnabled`.
- Atalho alternativo padrão: `Ctrl+Shift+S`.
- Prints são PNG comuns; nada é apagado de vez (apagar → Lixeira).
- Commits no formato Conventional Commits, **sem** linha `Co-Authored-By`.
- Código de portfólio: `cargo clippy --workspace -- -D warnings` e `cargo fmt --check` limpos ao final de cada task.

## Review Focus

1. **Dois prints no mesmo segundo** → o segundo vira `..._2.png`, nunca sobrescreve. (Task 1, Task 3)
2. **Nome de álbum inválido no Windows** (`a/b`, `con`, `x.`, vazio, só espaços) → erro claro, nada criado no disco. (Task 3)
3. **Pasta dos prints apagada/desconectada** (pendrive, pasta removida pelo usuário) → a próxima captura recria a pasta. Se não der pra gravar, o PNG vai pro fallback `%LOCALAPPDATA%\nahshot\unsaved\` e aparece uma notificação; o print não se perde. (Task 3, Task 10)
4. **Arquivos que não são PNG na pasta** (`.txt`, `desktop.ini`, `Thumbs.db`, `.PNG` maiúsculo) → não-PNG ignorado; `.PNG` aceito. (Task 4)
5. **Clique sem arrastar no overlay** (seleção 0×0 ou 1×1) → conta como "sem seleção": Esc cancela sem salvar e nada vai pra galeria. (Task 6)

---

## Estrutura de arquivos

```
Cargo.toml                         # workspace
crates/nahshot-core/
  Cargo.toml
  src/lib.rs                       # re-exports
  src/error.rs                     # CoreError, Result
  src/naming.rs                    # nomes de arquivo, pasta do mês, validação de álbum
  src/model.rs                     # Print, PrintId, Query, Filter, SortOrder
  src/db.rs                        # SQLite: schema, migrations, CRUD de metadados
  src/library.rs                   # Library: operações de alto nível (fs + db)
  src/sync.rs                      # reconciliação pasta ↔ banco
  src/watch.rs                     # watcher da pasta (notify)
  src/thumbs.rs                    # cache de miniaturas
crates/nahshot-capture/
  Cargo.toml
  src/lib.rs
  src/geom.rs                      # PxPoint, PxRect, Selection, handles
  src/annotate.rs                  # Tool, Shape, Annotations (undo)
  src/render.rs                    # compose desktop, crop, flatten (tiny-skia)
  src/grab.rs                      # captura dos monitores (xcap)
  src/font.rs                      # carrega Segoe UI (fontdue)
  src/overlay/mod.rs               # Overlay thread, OverlayHandle, CaptureResult
  src/overlay/window.rs            # janela por monitor (winit + softbuffer)
  src/overlay/toolbar.rs           # layout e hit-test da barra de ferramentas
  src/hotkey.rs                    # hook LL do PrintScreen + hotkey alternativo
  src/registry.rs                  # PrintScreenKeyForSnippingEnabled
src-tauri/
  Cargo.toml, tauri.conf.json, build.rs, capabilities/default.json
  src/main.rs                      # bootstrap, plugins, RunEvent
  src/settings.rs                  # Settings (JSON em app_config_dir)
  src/state.rs                     # AppState (Library, Settings, OverlayHandle)
  src/tray.rs                      # menu da bandeja
  src/capture_flow.rs              # CaptureResult → import → clipboard/salvar como
  src/commands.rs                  # comandos invocados pela UI
  src/windows.rs                   # abrir/fechar galeria, config, boas-vindas
ui/
  package.json, vite.config.ts, tsconfig.json
  gallery.html, settings.html, welcome.html
  src/api.ts                       # wrappers tipados de invoke()
  src/group.ts                     # agrupamento por dia (puro, testado)
  src/gallery.ts, src/viewer.ts, src/settings.ts, src/welcome.ts
  src/theme.css
  src/group.test.ts
scripts/measure-idle.ps1
README.md
```

Decisões fixas (o executor não troca):
- **Álbum não tem tabela no banco.** Álbum = diretório em `<root>\Álbuns\`; o álbum de um print é derivado do caminho dele.
- **Banco guarda caminho absoluto** (`prints.path UNIQUE`). O sync só remove registros cujo arquivo não existe mais, e só descobre arquivos novos dentro da raiz atual. Assim, prints de uma raiz antiga continuam aparecendo (spec 6.3).
- **`captured_at`**: na captura, a hora do momento. Na descoberta pelo sync, vem do nome do arquivo se casar com o padrão; senão, do mtime.
- Overlay e hooks rodam numa thread dedicada com o event loop do winit (`EventLoopBuilderExtWindows::with_any_thread(true)`), criada uma vez no boot e reaproveitada. As janelas do overlay (uma por monitor) ficam ocultas e só são mostradas na captura.

---

### Task 1: Workspace + nomes de arquivo

**Files:**
- Create: `Cargo.toml`, `crates/nahshot-core/Cargo.toml`, `crates/nahshot-core/src/{lib.rs,error.rs,naming.rs}`
- Test: testes unitários em `naming.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `pub enum CoreError { Io(std::io::Error), Db(rusqlite::Error), Image(image::ImageError), InvalidName(String), AlbumExists(String), AlbumNotFound(String), PrintNotFound(i64) }` (thiserror) e `pub type Result<T> = std::result::Result<T, CoreError>`
  - `pub fn file_name_for(t: chrono::NaiveDateTime) -> String` → `"2026-11-04_14-32-10.png"`
  - `pub fn parse_captured_at(file_name: &str) -> Option<NaiveDateTime>`: aceita `AAAA-MM-DD_HH-MM-SS.png` e `..._N.png`, case-insensitive na extensão
  - `pub fn month_dir(root: &Path, t: NaiveDateTime) -> PathBuf` → `root\2026-11`
  - `pub fn albums_dir(root: &Path) -> PathBuf` → `root\Álbuns`
  - `pub fn unique_path(dir: &Path, file_name: &str) -> PathBuf`: se existir, tenta `stem_2.ext`, `stem_3.ext`…
  - `pub fn validate_album_name(name: &str) -> Result<String>`: retorna o nome com trim
  - `pub fn validate_file_stem(stem: &str) -> Result<String>`: mesmas regras, usada no renomear

- [ ] **Step 1: Criar o workspace.** `Cargo.toml` raiz com `members = ["crates/*", "src-tauri"]` e `resolver = "2"`. Por enquanto só existe o core: comentar `src-tauri` até a Task 10. Deps do core: `chrono`, `thiserror`, `rusqlite = { features = ["bundled"] }`, `image = { default-features = false, features = ["png"] }`; dev: `tempfile`.

- [ ] **Step 2: Escrever os testes que falham**

```rust
#[test] fn file_name_format() {
    let t = NaiveDate::from_ymd_opt(2026,11,4).unwrap().and_hms_opt(14,32,10).unwrap();
    assert_eq!(file_name_for(t), "2026-11-04_14-32-10.png");
}
#[test] fn parse_roundtrip_and_suffix() {
    let t = parse_captured_at("2026-11-04_14-32-10_2.PNG").unwrap();
    assert_eq!(t.to_string(), "2026-11-04 14:32:10");
    assert!(parse_captured_at("foto.png").is_none());
}
#[test] fn unique_path_adds_suffix() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.png"), b"").unwrap();
    std::fs::write(d.path().join("a_2.png"), b"").unwrap();
    assert_eq!(unique_path(d.path(), "a.png"), d.path().join("a_3.png"));
}
#[test] fn album_name_rules() {
    for bad in ["", "   ", "a/b", "a\\b", "x:y", "q?", "con", "CON", "lpt1", "con.txt", "nome.", "a*b", "a<b", "a|b", "a\"b"] {
        assert!(validate_album_name(bad).is_err(), "{bad:?} deveria falhar");
    }
    assert_eq!(validate_album_name("  Trabalho ").unwrap(), "Trabalho");
    assert!(validate_album_name(&"x".repeat(101)).is_err());
    assert_eq!(validate_album_name("Ação & Bugs").unwrap(), "Ação & Bugs");
}
#[test] fn month_and_album_dirs() {
    let t = NaiveDate::from_ymd_opt(2026,1,9).unwrap().and_hms_opt(0,0,0).unwrap();
    assert_eq!(month_dir(Path::new("R"), t), Path::new("R").join("2026-01"));
    assert_eq!(albums_dir(Path::new("R")), Path::new("R").join("Álbuns"));
}
```

- [ ] **Step 3: Rodar e ver falhar.** `cargo test -p nahshot-core` → erro de compilação (funções não existem).

- [ ] **Step 4: Implementar `naming.rs` e `error.rs`.** Validação: `trim()` nas duas pontas e então rejeitar:
  - vazio;
  - qualquer `<>:"/\|?*` ou caractere de controle;
  - nome terminando em `.`;
  - nomes reservados, case-insensitive, com ou sem extensão: `CON PRN AUX NUL COM1-9 LPT1-9`;
  - mais de 100 caracteres.

- [ ] **Step 5: Rodar e ver passar.** `cargo test -p nahshot-core` → todos PASS.

- [ ] **Step 6: Commit.** `git add -A && git commit -m "feat(core): workspace and file naming rules"`

---

### Task 2: Banco de metadados

**Files:**
- Create: `crates/nahshot-core/src/{model.rs,db.rs}`
- Test: `db.rs` (testes com `Db::open_in_memory()`)

**Interfaces:**
- Consumes: `CoreError`, `Result` (Task 1)
- Produces:
  - `pub struct PrintId(pub i64)` (Copy, Eq, Hash, Serialize, Deserialize; serializa como número)
  - `pub struct PrintRow { pub id: PrintId, pub path: PathBuf, pub captured_at: NaiveDateTime, pub favorite: bool, pub tags: Vec<String> }`
  - `pub struct Db` com:
    - `open(path: &Path) -> Result<Db>`, `open_in_memory() -> Result<Db>`
    - `insert(path: &Path, captured_at: NaiveDateTime) -> Result<PrintId>`: `INSERT OR IGNORE` + retorna id existente se o path já existe
    - `get(id) -> Result<PrintRow>` (`PrintNotFound` se não existe), `find_by_path(&Path) -> Result<Option<PrintId>>`
    - `all() -> Result<Vec<PrintRow>>`
    - `update_path(id, &Path) -> Result<()>`, `remove(id) -> Result<()>`
    - `set_favorite(id, bool)`, `add_tag(id, &str)`, `remove_tag(id, &str)`, `tags() -> Result<Vec<String>>` (só etiquetas usadas por algum print, ordem alfabética)
  - Schema v1 (`PRAGMA user_version = 1`):
    - `prints(id INTEGER PK, path TEXT NOT NULL UNIQUE COLLATE NOCASE, captured_at TEXT NOT NULL, favorite INTEGER NOT NULL DEFAULT 0)`
    - `tags(id INTEGER PK, name TEXT NOT NULL UNIQUE COLLATE NOCASE)`
    - `print_tags(print_id REFERENCES prints ON DELETE CASCADE, tag_id REFERENCES tags ON DELETE CASCADE, PRIMARY KEY(print_id, tag_id))`
  - `PRAGMA foreign_keys = ON; journal_mode = WAL`.

- [ ] **Step 1: Testes que falham**

```rust
#[test] fn insert_is_idempotent_by_path_case_insensitive() {
    let db = Db::open_in_memory().unwrap();
    let a = db.insert(Path::new(r"C:\x\A.png"), t0()).unwrap();
    let b = db.insert(Path::new(r"c:\x\a.png"), t0()).unwrap();
    assert_eq!(a, b);
}
#[test] fn tags_and_favorite() {
    let db = Db::open_in_memory().unwrap();
    let id = db.insert(Path::new("p.png"), t0()).unwrap();
    db.add_tag(id, "bug").unwrap(); db.add_tag(id, "BUG").unwrap(); db.add_tag(id, "figma").unwrap();
    db.set_favorite(id, true).unwrap();
    let r = db.get(id).unwrap();
    assert!(r.favorite);
    assert_eq!(r.tags, vec!["bug", "figma"]);
    db.remove_tag(id, "bug").unwrap();
    assert_eq!(db.tags().unwrap(), vec!["figma"]);
}
#[test] fn remove_cascades_tags() {
    let db = Db::open_in_memory().unwrap();
    let id = db.insert(Path::new("p.png"), t0()).unwrap();
    db.add_tag(id, "x").unwrap();
    db.remove(id).unwrap();
    assert!(db.tags().unwrap().is_empty());
    assert!(matches!(db.get(id), Err(CoreError::PrintNotFound(_))));
}
#[test] fn reopen_keeps_data() { /* Db::open em tempdir, insert, drop, reabre, all().len() == 1 */ }
```

- [ ] **Step 2: Rodar e ver falhar.** `cargo test -p nahshot-core db::` → falha de compilação.
- [ ] **Step 3: Implementar `model.rs` (PrintId) e `db.rs`.** `captured_at` em ISO `%Y-%m-%dT%H:%M:%S`. `Db` envolve `rusqlite::Connection`; métodos recebem `&self`.
- [ ] **Step 4: Rodar e ver passar.** `cargo test -p nahshot-core` → PASS.
- [ ] **Step 5: Commit.** `feat(core): sqlite metadata store`

---

### Task 3: Library: captura, renomear, álbuns, apagar, consulta

**Files:**
- Create: `crates/nahshot-core/src/library.rs`
- Modify: `crates/nahshot-core/src/model.rs` (Print, Query, Filter, SortOrder)
- Test: `crates/nahshot-core/tests/library.rs` (integração com tempdir)

**Interfaces:**
- Consumes: `naming::*` (Task 1), `Db`, `PrintRow`, `PrintId` (Task 2)
- Produces:
  - `#[derive(Serialize)] pub struct Print { pub id: PrintId, pub path: PathBuf, pub file_name: String, pub captured_at: NaiveDateTime, pub favorite: bool, pub album: Option<String>, pub tags: Vec<String> }`
  - `pub enum Filter { All, Favorites, Album(String), Tag(String) }`, `pub enum SortOrder { NewestFirst, OldestFirst }`, `pub struct Query { pub filter: Filter, pub search: Option<String>, pub order: SortOrder }` (todos `Deserialize`, serde `rename_all = "camelCase"`; enums `tag = "kind", content = "value"`)
  - `pub struct Library` com:
    - `open(root: PathBuf, db_path: &Path) -> Result<Library>`, `root(&self) -> &Path`
    - `import_capture(&self, png: &[u8], at: NaiveDateTime) -> Result<Print>`: cria `month_dir` se faltar (`create_dir_all`), `unique_path`, grava, `db.insert`
    - `query(&self, q: &Query) -> Result<Vec<Print>>`
    - `get(&self, id) -> Result<Print>`
    - `rename(&self, id, new_stem: &str) -> Result<Print>`: valida, renomeia no disco no mesmo diretório (`unique_path`), atualiza path
    - `set_favorite(&self, id, bool)`, `add_tag(&self, id, &str)`, `remove_tag(&self, id, &str)`, `tags(&self) -> Result<Vec<String>>`
    - `albums(&self) -> Result<Vec<String>>`: subdiretórios de `albums_dir(root)`, ordem alfabética; vazio se a pasta não existe
    - `create_album(&self, name) -> Result<String>` (`AlbumExists` se já existe)
    - `rename_album(&self, old, new) -> Result<String>`: renomeia o diretório e atualiza o `path` de cada print dele no banco
    - `delete_album(&self, name) -> Result<()>`: move cada PNG para `month_dir(captured_at)` e depois remove o diretório **somente se vazio** (`fs::remove_dir`, não `remove_dir_all`); se sobrar arquivo não-PNG, deixa a pasta lá
    - `move_to_album(&self, ids: &[PrintId], album: &str) -> Result<()>`, `remove_from_album(&self, ids: &[PrintId]) -> Result<()>` (volta pro `month_dir`)
    - `delete(&self, ids: &[PrintId]) -> Result<()>`: `trash::delete(path)` e depois `db.remove`
    - `album_of(root: &Path, path: &Path) -> Option<String>` (fn livre): `Some(nome)` se `path.parent() == albums_dir(root).join(nome)`
- Movimentação de arquivo (`move_file(from, to_dir) -> Result<PathBuf>`): `unique_path` + `fs::rename`; se der erro de cross-device, `copy` + `remove_file`.
- Busca: case-insensitive, casa se o termo está em `file_name`, em alguma tag ou em `album`.
- Fallback "não perder print" fica no app (Task 10). Aqui `import_capture` só precisa recriar a pasta e devolver `Err` se a gravação falhar.

- [ ] **Step 1: Testes que falham** (helper `lib()` cria tempdir com `root` e `db.sqlite`; `png()` gera um PNG 2×2 válido via `image`)

```rust
#[test] fn import_goes_to_month_folder_and_never_overwrites() {
    let (_d, lib) = lib();
    let a = lib.import_capture(&png(), t("2026-11-04 14:32:10")).unwrap();
    let b = lib.import_capture(&png(), t("2026-11-04 14:32:10")).unwrap();
    assert_eq!(a.path, lib.root().join("2026-11").join("2026-11-04_14-32-10.png"));
    assert_eq!(b.file_name, "2026-11-04_14-32-10_2.png");
    assert!(a.path.exists() && b.path.exists());
}
#[test] fn import_recreates_deleted_root() {
    let (_d, lib) = lib();
    std::fs::remove_dir_all(lib.root()).ok();
    assert!(lib.import_capture(&png(), t0()).unwrap().path.exists());
}
#[test] fn album_move_is_a_real_folder_move() {
    let (_d, lib) = lib();
    let p = lib.import_capture(&png(), t0()).unwrap();
    lib.create_album("Bugs").unwrap();
    lib.move_to_album(&[p.id], "Bugs").unwrap();
    let p2 = lib.get(p.id).unwrap();
    assert_eq!(p2.album.as_deref(), Some("Bugs"));
    assert_eq!(p2.path, lib.root().join("Álbuns").join("Bugs").join(&p.file_name));
    lib.remove_from_album(&[p.id]).unwrap();
    assert_eq!(lib.get(p.id).unwrap().path, p.path);
}
#[test] fn move_into_album_with_same_name_file_gets_suffix() { /* dois prints mesmo nome em meses distintos → ambos no álbum, um com _2 */ }
#[test] fn invalid_album_name_creates_nothing() {
    let (_d, lib) = lib();
    assert!(matches!(lib.create_album("a/b"), Err(CoreError::InvalidName(_))));
    assert!(matches!(lib.create_album("con"), Err(CoreError::InvalidName(_))));
    assert!(lib.albums().unwrap().is_empty());
}
#[test] fn rename_album_updates_paths() { /* create "A", move print, rename_album("A","B") → print.album == Some("B"), path existe */ }
#[test] fn delete_album_returns_prints_to_month_folder() {
    /* print em "Bugs" → delete_album("Bugs") → print existe em root/2026-11, pasta Bugs não existe, albums() vazio */
}
#[test] fn rename_renames_file_on_disk() {
    /* rename(id, "tela de login") → file_name "tela de login.png", arquivo antigo não existe; rename(id, "a?b") → InvalidName */
}
#[test] fn query_filters_search_and_order() {
    /* 3 prints em horários t1<t2<t3; t2 favorito; t3 tag "figma"; t1 no álbum "Trabalho"
       All+NewestFirst → [t3,t2,t1]; OldestFirst → [t1,t2,t3]
       Favorites → [t2]; Tag("figma") → [t3]; Album("Trabalho") → [t1]
       search "FIG" → [t3]; search "trab" → [t1]; search "2026-11-04" → todos */
}
#[test] fn delete_removes_from_gallery() { /* delete(&[id]) → get(id) == PrintNotFound, arquivo não existe no local original */ }
```

- [ ] **Step 2: Rodar e ver falhar.** `cargo test -p nahshot-core --test library` → falha de compilação.
- [ ] **Step 3: Implementar `model.rs` (tipos) e `library.rs`** com as assinaturas acima. `Library` guarda `root: PathBuf` e `db: Db`.
- [ ] **Step 4: Rodar e ver passar.** `cargo test -p nahshot-core` → PASS.
- [ ] **Step 5: Commit.** `feat(core): library operations with folder-backed albums`

---

### Task 4: Sync pasta ↔ banco + watcher

**Files:**
- Create: `crates/nahshot-core/src/{sync.rs,watch.rs}`
- Test: `crates/nahshot-core/tests/sync.rs`

**Interfaces:**
- Consumes: `Library`, `Db`, `naming::parse_captured_at` (Tasks 1–3)
- Produces:
  - `#[derive(Debug, Default, PartialEq)] pub struct SyncReport { pub added: usize, pub removed: usize }`
  - `impl Library { pub fn sync(&self) -> Result<SyncReport> }`:
    1. Remove do banco todo registro cujo `path` não existe.
    2. Percorre recursivamente a raiz (`walkdir`, profundidade máxima 3: raiz/mês, raiz/Álbuns/nome). Aceita arquivos com extensão `png` case-insensitive e insere os que faltam, com `captured_at` = `parse_captured_at(nome)` ou, se não casar, o mtime local.
  - `pub struct LibraryWatcher` (guarda o debouncer; drop = para de observar)
  - `pub fn watch(root: &Path, on_change: impl Fn() + Send + 'static) -> Result<LibraryWatcher>`: `notify-debouncer-mini` com 500 ms, recursivo. Chama `on_change` uma vez por rajada. O app reage com `sync()` e evento pra UI.

- [ ] **Step 1: Testes que falham**

```rust
#[test] fn sync_adds_png_from_explorer_and_ignores_other_files() {
    let (_d, lib) = lib();
    let m = lib.root().join("2026-10"); std::fs::create_dir_all(&m).unwrap();
    std::fs::write(m.join("2026-10-01_08-00-00.png"), png()).unwrap();
    std::fs::write(m.join("FOTO.PNG"), png()).unwrap();
    std::fs::write(m.join("notas.txt"), b"x").unwrap();
    std::fs::write(m.join("desktop.ini"), b"x").unwrap();
    std::fs::write(m.join("Thumbs.db"), b"x").unwrap();
    assert_eq!(lib.sync().unwrap(), SyncReport { added: 2, removed: 0 });
    let all = lib.query(&Query::default()).unwrap();
    assert!(all.iter().any(|p| p.captured_at.to_string() == "2026-10-01 08:00:00"));
}
#[test] fn sync_removes_orphans_and_keeps_tags_of_survivors() { /* 2 prints, tag em ambos, apaga 1 do disco → removed 1, sobrevivente mantém tag */ }
#[test] fn sync_keeps_prints_outside_current_root() {
    /* importa em rootA; Library reaberta com rootB e mesmo db → sync() não remove o print de rootA */
}
#[test] fn album_folder_created_in_explorer_shows_up() { /* mkdir root/Álbuns/Memes → albums() contém "Memes" sem sync */ }
#[test] fn sync_is_idempotent() { /* sync duas vezes → segunda retorna SyncReport::default() */ }
#[test] fn watcher_fires_on_new_file() {
    /* watch(root, cb que envia em mpsc) → grava png → recv_timeout(5s) Ok */
}
```

`Query::default()` = `All`, sem busca, `NewestFirst`: adicionar `impl Default` em `model.rs`.

- [ ] **Step 2: Rodar e ver falhar.** `cargo test -p nahshot-core --test sync`
- [ ] **Step 3: Implementar `sync.rs` e `watch.rs`.**
- [ ] **Step 4: Rodar e ver passar.** `cargo test -p nahshot-core` → PASS.
- [ ] **Step 5: Commit.** `feat(core): folder sync and watcher`

---

### Task 5: Cache de miniaturas

**Files:**
- Create: `crates/nahshot-core/src/thumbs.rs`
- Test: unitários em `thumbs.rs`

**Interfaces:**
- Produces:
  - `pub struct ThumbCache { dir: PathBuf }`, `ThumbCache::new(dir: PathBuf) -> Result<Self>`
  - `pub fn get_or_create(&self, source: &Path) -> Result<PathBuf>`: JPEG com qualidade 80 e lado maior de **320 px** (sem upscale), filtro `Triangle`. Chave = hash (`std::hash` → hex) de `path absoluto em minúsculas + mtime + tamanho`. Se a fonte muda ou é renomeada, gera de novo. Arquivo em `dir/<hash>.jpg`.
  - `pub fn prune(&self, keep: &HashSet<PathBuf>) -> Result<usize>`: apaga do cache as miniaturas que não estão em `keep`; retorna quantas apagou.
- Dep: `image` com feature `jpeg`.

- [ ] **Step 1: Testes que falham.** `thumb_is_small_and_cached`: PNG 1920×1080 → thumb 320×180, e a segunda chamada devolve o mesmo path sem regravar (mtime da thumb igual). `thumb_regenerates_when_source_changes`: regrava a fonte com outro tamanho → path diferente. `small_images_are_not_upscaled`: 100×50 → 100×50. `prune_deletes_unknown`.
- [ ] **Step 2: Rodar e ver falhar.** `cargo test -p nahshot-core thumbs`
- [ ] **Step 3: Implementar.**
- [ ] **Step 4: Rodar e ver passar.**
- [ ] **Step 5: Commit.** `feat(core): thumbnail cache`

---

### Task 6: Geometria da seleção + modelo de anotações

**Files:**
- Create: `crates/nahshot-capture/Cargo.toml`, `src/{lib.rs,geom.rs,annotate.rs}`
- Test: unitários nos dois módulos

**Interfaces:**
- Produces (`geom.rs`; coordenadas em **pixels físicos do desktop virtual**, i32, podem ser negativas):
  - `pub struct PxPoint { pub x: i32, pub y: i32 }`, `pub struct PxRect { pub x: i32, pub y: i32, pub w: u32, pub h: u32 }` com `contains(PxPoint)`, `intersect(&PxRect) -> Option<PxRect>`
  - `pub const MIN_SELECTION: u32 = 3;`
  - `pub enum Handle { Move, N, S, E, W, NE, NW, SE, SW }`
  - `pub struct Selection` com `begin(p)`, `update(p)` (durante o arrasto inicial), `rect() -> Option<PxRect>` (`None` se `w < MIN_SELECTION || h < MIN_SELECTION`), `hit(p, tolerance: i32) -> Option<Handle>` e `drag(handle, from: PxPoint, to: PxPoint)` (mover/redimensionar; inverter a borda normaliza o retângulo).
- Produces (`annotate.rs`; coordenadas **relativas ao canto da seleção**, `f32`):
  - `pub enum Tool { Pen, Line, Arrow, Rect, Ellipse, Marker, Text }`
  - `pub struct Color(pub u8, pub u8, pub u8)`, `pub const PALETTE: [Color; 8]` (vermelho `#E53935` padrão, laranja, amarelo, verde, azul, roxo, preto, branco)
  - `pub enum Shape { Pen { pts: Vec<(f32,f32)>, color: Color, width: f32 }, Line { a, b, color, width }, Arrow { a, b, color, width }, Rect { a, b, color, width }, Ellipse { a, b, color, width }, Marker { pts, color }, Text { at: (f32,f32), text: String, color: Color, size: f32 } }`
  - `pub struct Annotations { shapes: Vec<Shape> }`: `push`, `undo() -> Option<Shape>`, `shapes() -> &[Shape]`, `is_empty()`
  - Espessuras: traço 3 px; marca-texto 18 px com alpha 0.35; texto 20 px. Todos multiplicados pela escala do monitor onde o shape começou (`scale` passado por quem cria o shape).

- [ ] **Step 1: Testes que falham**

```rust
#[test] fn click_without_drag_is_no_selection() {
    let mut s = Selection::default();
    s.begin(PxPoint{x:10,y:10}); s.update(PxPoint{x:11,y:11});
    assert_eq!(s.rect(), None);
}
#[test] fn drag_any_direction_normalizes() {
    let mut s = Selection::default();
    s.begin(PxPoint{x:100,y:100}); s.update(PxPoint{x:-20,y:40});
    assert_eq!(s.rect(), Some(PxRect{x:-20,y:40,w:120,h:60}));
}
#[test] fn hit_and_resize_handles() {
    // rect (0,0,100,100): hit((100,100),4)==SE; hit((50,50),4)==Move; hit((300,300),4)==None
    // drag(SE,(100,100),(150,120)) → (0,0,150,120); drag(Move,(50,50),(60,70)) → (10,20,150,120)
    // drag(W,(10,50),(200,50)) → borda passa da direita → rect normalizado (160,20,40,120)
}
#[test] fn undo_pops_last_shape() { /* push 2, undo → 1, undo → 0, undo → None */ }
```

- [ ] **Step 2: Rodar e ver falhar.** `cargo test -p nahshot-capture`
- [ ] **Step 3: Implementar.** Deps do crate: `image`, `tiny-skia`, `fontdue`, `xcap`, `winit = "0.30"`, `softbuffer`, `windows` (features `Win32_UI_WindowsAndMessaging`, `Win32_Foundation`, `Win32_UI_Input_KeyboardAndMouse`, `Win32_System_LibraryLoader`), `winreg`, `global-hotkey`, `log`, `thiserror`.
- [ ] **Step 4: Rodar e ver passar.**
- [ ] **Step 5: Commit.** `feat(capture): selection geometry and annotation model`

---

### Task 7: Render: compor monitores, recortar, achatar anotações

**Files:**
- Create: `crates/nahshot-capture/src/{render.rs,font.rs,grab.rs}`
- Test: unitários em `render.rs`

**Interfaces:**
- Consumes: `PxRect`, `Annotations`, `Shape` (Task 6)
- Produces:
  - `pub struct MonitorShot { pub id: u32, pub origin: PxPoint, pub scale: f32, pub image: image::RgbaImage }` (origin e tamanho em px físicos)
  - `grab.rs`: `pub fn grab_all() -> Result<Vec<MonitorShot>, CaptureError>` via `xcap::Monitor::all()` + `capture_image()`. Captura os monitores em paralelo (`std::thread::scope`).
  - `pub struct Desktop { pub origin: PxPoint, pub image: RgbaImage }`, `pub fn compose(shots: &[MonitorShot]) -> Desktop`: bounding box de todos os monitores, com as áreas sem monitor pretas
  - `pub fn crop(desktop: &Desktop, r: PxRect) -> RgbaImage` (`r` em coordenadas do desktop virtual; parte fora é recortada fora)
  - `pub fn flatten(base: &RgbaImage, ann: &Annotations, font: &Font) -> RgbaImage`: rasteriza com tiny-skia (linhas e setas com cap redondo, ponta de seta com triângulo de lado = 4× a espessura, elipse via `PathBuilder::push_oval`, marca-texto com alpha). Texto via fontdue, desenhado direto no pixmap.
  - `pub fn draw_shapes(pixmap: &mut tiny_skia::PixmapMut, offset: (f32,f32), ann: &Annotations, font: &Font)`: mesma rotina, reaproveitada pelo overlay ao vivo
  - `font.rs`: `pub struct Font(fontdue::Font)`, `Font::system() -> Font`: tenta `C:\Windows\Fonts\segoeui.ttf` e depois `arial.ttf`

- [ ] **Step 1: Testes que falham**

```rust
#[test] fn compose_two_monitors_with_negative_origin() {
    // shot A origin(-1920,0) 1920x1080 vermelho; B origin(0,0) 2560x1440 azul (escala 1.5)
    // Desktop.origin == (-1920,0); image 4480x1440; pixel (0,0) vermelho; (1920,0) azul; (0,1200) preto
}
#[test] fn crop_across_monitors() {
    // crop rect x:-10,y:0,w:20,h:10 → 20x10; col 0..10 vermelho, 10..20 azul
}
#[test] fn flatten_draws_rect_outline_in_color() {
    // base branca 100x100; Rect a(10,10) b(90,90) vermelho width 3 → pixel (10,50) vermelho, (50,50) branco
}
#[test] fn flatten_empty_annotations_is_identity() { /* bytes iguais */ }
```

- [ ] **Step 2: Rodar e ver falhar.** `cargo test -p nahshot-capture render`
- [ ] **Step 3: Implementar.**
- [ ] **Step 4: Rodar e ver passar.**
- [ ] **Step 5: Commit.** `feat(capture): desktop compose, crop and annotation rasterizer`

---

### Task 8: Overlay nativo

**Files:**
- Create: `crates/nahshot-capture/src/overlay/{mod.rs,window.rs,toolbar.rs}`
- Create: `crates/nahshot-capture/examples/overlay.rs` (harness manual)
- Test: unitários em `toolbar.rs` (layout e hit-test)

**Interfaces:**
- Consumes: `grab_all`, `compose`, `crop`, `flatten`, `draw_shapes`, `Font` (Task 7); `Selection`, `Annotations`, `Tool` (Task 6)
- Produces:
  - `pub enum Action { Copy, SaveAs, Close }`
  - `pub struct CaptureResult { pub action: Action, pub image: Option<RgbaImage>, pub captured_at: chrono::NaiveDateTime }`: `image == None` só em `Close` sem seleção válida
  - `pub struct OverlayHandle` (Clone, Send) com `trigger(&self)` (envia `UserEvent::Capture` via `EventLoopProxy`) e `set_hotkeys(&self, cfg: HotkeyConfig)` (Task 9)
  - `pub fn spawn_overlay(on_result: impl Fn(CaptureResult) + Send + 'static) -> OverlayHandle`: cria a thread, o `EventLoop<UserEvent>` com `with_any_thread(true)` e roda pra sempre
  - `toolbar.rs`: `pub enum ToolbarItem { Tool(Tool), Color(usize), Undo, Copy, SaveAs, Close }`, `pub fn layout(sel: PxRect, screen: PxRect, scale: f32) -> Vec<(ToolbarItem, PxRect)>`: barra horizontal abaixo da seleção (botões de 32×32 lógicos × `scale`, 4 px de gap); vai pra cima se não couber embaixo; vai pra dentro, no canto inferior da seleção, se não couber em nenhum dos dois. `pub fn hit(items: &[(ToolbarItem, PxRect)], p: PxPoint) -> Option<ToolbarItem>`

Comportamento (o executor implementa; verificação manual no Step 6):
- `Capture`: registra `Instant`, chama `grab_all()` e `compose()`, guarda o `Desktop` em memória. Para cada monitor: reaproveita a janela já criada desse monitor (sem decoração, topmost, `skip_taskbar`, posição e tamanho físicos do monitor) ou cria se for novo, pinta o frame (screenshot do monitor + véu preto alpha 0.45) e chama `set_visible(true)` + foco. Loga `info!("overlay_visible_ms={}", t.elapsed())` no primeiro `present`.
- O ponteiro chega em coordenadas da janela; converter para o desktop virtual somando o `origin` do monitor (as janelas usam px físicos, então não há divisão por escala). O estado (`Selection`, `Annotations`, ferramenta e cor atuais) é **único e compartilhado** entre as janelas, então a seleção atravessa monitores. Cada janela redesenha só a sua fatia.
- Desenho por frame: screenshot com véu; seleção sem véu e com borda de 1 px; texto do tamanho `"{w} × {h}"` acima da seleção; shapes via `draw_shapes`; barra de ferramentas (ícones desenhados com primitivas tiny-skia: linhas, setas e formas; letras "T" e "↶" via fontdue).
- Teclas: `Esc` → `Close`; `Ctrl+C` → `Copy`; `Ctrl+S` → `SaveAs`; `Ctrl+Z` → undo. Com a ferramenta Texto ativa: clique cria uma caixa e o texto chega por `WindowEvent::Ime`/`KeyboardInput.text`; `Enter` confirma; `Esc` dentro do texto cancela só o texto.
- Sem ferramenta ativa: arrastar fora da seleção cria uma nova seleção; arrastar sobre ela usa `Selection::hit` pra mover/redimensionar. Com ferramenta ativa e clique dentro da seleção: desenha.
- Ao finalizar: monta `CaptureResult` (com `rect` válido: `flatten(crop(desktop, rect), ann)`), esconde todas as janelas (`set_visible(false)`), **libera o `Desktop` e os buffers** (memória idle) e chama `on_result`.
- Um `Capture` disparado enquanto o overlay já está aberto é ignorado.

- [ ] **Step 1: Testes de toolbar que falham.** `toolbar_below_selection` (sel no meio da tela → todos os itens com `y > sel.y + sel.h`), `toolbar_flips_above_at_bottom_edge`, `toolbar_goes_inside_when_selection_fills_screen`, `hit_returns_item` (centro do 1º rect → `Tool(Pen)`), `layout_scales_with_dpi` (scale 1.5 → botão de 48 px).
- [ ] **Step 2: Rodar e ver falhar.** `cargo test -p nahshot-capture toolbar`
- [ ] **Step 3: Implementar `toolbar.rs` e rodar os testes até passarem.**
- [ ] **Step 4: Implementar `overlay/mod.rs` e `window.rs`.**
- [ ] **Step 5: Criar `examples/overlay.rs`.** Faz `spawn_overlay`, chama `trigger()` a cada Enter no stdin e, ao receber o resultado, salva `out-N.png` e imprime a ação.
- [ ] **Step 6: Verificação manual.** Rodar `cargo run -p nahshot-capture --example overlay --release` e checar:
  - overlay aparece em todos os monitores;
  - log mostra `overlay_visible_ms` < 150;
  - seleção atravessando dois monitores sai num PNG contínuo;
  - num monitor a 150%, o PNG tem tamanho em px físicos e os traços não ficam deslocados do cursor;
  - todas as ferramentas desenham;
  - Ctrl+Z desfaz;
  - Esc sem seleção imprime `Close` com `image: None`;
  - clique sem arrastar + Esc também dá `None`;
  - Esc com seleção dá `Close` com imagem.
  Se só houver um monitor disponível, registrar no commit quais itens ficaram sem verificar.
- [ ] **Step 7: Commit.** `feat(capture): native multi-monitor overlay`

---

### Task 9: Hotkeys + chave de registro do Print Screen

**Files:**
- Create: `crates/nahshot-capture/src/{hotkey.rs,registry.rs}`
- Modify: `crates/nahshot-capture/src/overlay/mod.rs` (instalar/atualizar hotkeys na thread do overlay)
- Test: unitários em `registry.rs` (usando uma chave de teste) e `hotkey.rs` (parse)

**Interfaces:**
- Produces:
  - `pub struct HotkeyConfig { pub intercept_print_screen: bool, pub alt: Option<String> }`, com `alt` no formato `"Ctrl+Shift+S"`
  - `pub fn parse_accelerator(s: &str) -> Result<global_hotkey::hotkey::HotKey, CaptureError>`: aceita `Ctrl|Alt|Shift|Win` + tecla (A–Z, 0–9, F1–F24, `PrintScreen`)
  - `intercept_print_screen = true`: instala `SetWindowsHookExW(WH_KEYBOARD_LL)` na thread do overlay. No `VK_SNAPSHOT` sem modificadores, em key-down, posta `UserEvent::Capture` e **retorna 1** (bloqueia o Windows); no key-up também retorna 1. As outras teclas seguem com `CallNextHookEx`. `false` → desinstala o hook.
  - `alt`: `GlobalHotKeyManager` registrado na thread do overlay. Os eventos de `GlobalHotKeyEvent::receiver()` viram `UserEvent::Capture`. Trocar a config desregistra o anterior.
  - `registry.rs`:
    - `pub const KEYBOARD_KEY: &str = r"Control Panel\Keyboard"; pub const SNIP_VALUE: &str = "PrintScreenKeyForSnippingEnabled";`
    - `pub fn read_snipping(key_path: &str) -> io::Result<Option<u32>>` (HKCU; `None` se o valor não existe)
    - `pub fn write_snipping(key_path: &str, v: Option<u32>) -> io::Result<()>` (`None` apaga o valor)

- [ ] **Step 1: Testes que falham**

```rust
const TEST_KEY: &str = r"Software\nahshot-test\Keyboard";
#[test] fn registry_roundtrip() {
    write_snipping(TEST_KEY, Some(0)).unwrap();
    assert_eq!(read_snipping(TEST_KEY).unwrap(), Some(0));
    write_snipping(TEST_KEY, None).unwrap();
    assert_eq!(read_snipping(TEST_KEY).unwrap(), None);
    // cleanup: apagar HKCU\Software\nahshot-test
}
#[test] fn parse_accelerators() {
    assert!(parse_accelerator("Ctrl+Shift+S").is_ok());
    assert!(parse_accelerator("PrintScreen").is_ok());
    assert!(parse_accelerator("Ctrl+").is_err());
    assert!(parse_accelerator("Banana").is_err());
}
```

- [ ] **Step 2: Rodar e ver falhar.** `cargo test -p nahshot-capture registry hotkey`
- [ ] **Step 3: Implementar.**
- [ ] **Step 4: Rodar e ver passar.**
- [ ] **Step 5: Verificação manual.** Estender `examples/overlay.rs` com o argumento `--intercept`.
  - Com ele, Print Screen abre o overlay e o clipboard **não** recebe a tela inteira (colar no Paint não cola nada novo).
  - Sem ele, Ctrl+Shift+S abre e Print Screen continua sendo do Windows.
- [ ] **Step 6: Commit.** `feat(capture): print screen hook, alt hotkey and snipping registry toggle`

---

### Task 10: App Tauri: bandeja, configurações, fluxo de captura

**Files:**
- Create: `src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json}`, `src-tauri/src/{main.rs,settings.rs,state.rs,tray.rs,capture_flow.rs,windows.rs}`, `src-tauri/icons/*` (gerar com `cargo tauri icon` a partir de um PNG simples)
- Modify: `Cargo.toml` (descomentar `src-tauri`)
- Test: unitários em `settings.rs` e `capture_flow.rs`
- Nome do pacote do app: `nahshot` (binário `nahshot.exe`, identifier `dev.nathangds.nahshot`).

**Interfaces:**
- Consumes: `Library`, `ThumbCache`, `watch` (core); `spawn_overlay`, `OverlayHandle`, `CaptureResult`, `Action`, `HotkeyConfig`, `read_snipping/write_snipping` (capture)
- Produces:
  - `#[derive(Serialize, Deserialize, Clone)] #[serde(rename_all = "camelCase", default)] pub struct Settings { pub prints_dir: PathBuf, pub autostart: bool, pub default_print_screen: bool, pub alt_hotkey: String, pub theme: Theme, pub first_run_done: bool, pub saved_snipping_value: Option<u32> }`
    - Padrões: `prints_dir = <Pictures>\nahshot`, `autostart = true`, `default_print_screen = false`, `alt_hotkey = "Ctrl+Shift+S"`, `theme = System`, `first_run_done = false`
    - `pub enum Theme { System, Light, Dark }`
  - `Settings::load(path: &Path) -> Settings`: arquivo ausente ou corrompido → padrão; corrompido gera `warn!`. `Settings::save(&self, path: &Path) -> io::Result<()>`: escrita atômica, `tmp` + `rename`.
  - `pub struct AppState { pub lib: Mutex<Library>, pub thumbs: ThumbCache, pub settings: Mutex<Settings>, pub overlay: OverlayHandle, pub watcher: Mutex<Option<LibraryWatcher>> }`
  - `capture_flow.rs`: `pub fn handle(app: &AppHandle, r: CaptureResult)`. Sem imagem → nada.
    1. Codifica a imagem em PNG.
    2. `lib.import_capture`. Se der erro: grava em `<app_local_data_dir>\unsaved\<file_name_for(at)>` e mostra notificação "Não foi possível salvar na pasta dos prints; o print foi guardado em …".
    3. Conforme a ação: `Copy` → `arboard` set_image. `SaveAs` → `tauri-plugin-dialog` save dialog com nome sugerido `file_name_for(at)` e filtro PNG, gravando a cópia. `Close` → nada além do import.
    4. Emite o evento `library-changed` para as janelas abertas.
  - `pub fn encode_png(img: &RgbaImage) -> Vec<u8>` (testado: decodifica de volta com as mesmas dimensões)
  - `pub fn apply_print_screen_default(enable: bool, settings: &mut Settings, overlay: &OverlayHandle) -> io::Result<()>`:
    - ligar → `saved_snipping_value = read_snipping(KEYBOARD_KEY)`, `write_snipping(Some(0))`, `overlay.set_hotkeys({intercept: true, alt})`
    - desligar → `write_snipping(saved_snipping_value)`, `intercept: false`
- `main.rs`:
  - Plugins: `tauri-plugin-single-instance` (segunda instância → abre a galeria), `tauri-plugin-autostart` (aplica `settings.autostart` no boot), `tauri-plugin-dialog`, `tauri-plugin-notification`.
  - `tauri.conf.json`: `"app": { "windows": [] }`; asset protocol habilitado com escopo `$APPLOCALDATA/thumbs/**`.
  - No setup:
    1. Carrega `Settings` de `app_config_dir/settings.json`.
    2. Abre `Library(prints_dir, app_local_data_dir/nahshot.db)`, roda `sync()` numa thread e cria o `ThumbCache(app_local_data_dir/thumbs)`.
    3. Chama `spawn_overlay(move |r| capture_flow::handle(&app, r))`.
    4. Aplica `HotkeyConfig` a partir das settings.
    5. Inicia `watch(prints_dir, …)`, que faz `sync` e emite `library-changed`.
    6. Monta a bandeja.
    7. Se `!first_run_done`, abre a janela de boas-vindas.
  - `RunEvent::ExitRequested { api, code: None, .. }` → `api.prevent_exit()`: fechar a última janela não encerra o app.
- `tray.rs`: menu "Capturar" (`overlay.trigger()`), "Abrir galeria", "Configurações", separador, "Sair" (`app.exit(0)`). Clique esquerdo no ícone → abrir galeria.
- `windows.rs`: `open_gallery(app)`, `open_settings(app)`, `open_welcome(app)`. Se a janela com aquele label já existe, só dá foco. Senão cria com `WebviewWindowBuilder` (`gallery.html` 1100×720, `settings.html` 560×520, `welcome.html` 520×420). Ao fechar, a janela é **destruída** (comportamento padrão; não interceptar `CloseRequested` pra esconder).

- [ ] **Step 1: Testes que falham.**
  - `settings_defaults_when_missing`.
  - `settings_defaults_when_corrupt`: arquivo com `"{lixo"`.
  - `settings_roundtrip_and_unknown_fields_default`: JSON só com `{"theme":"dark"}` carrega com os outros campos no padrão.
  - `encode_png_roundtrip`.
  - `print_screen_default_restores_previous_value`: usa um `key_path` injetável. Assinatura de teste `apply_print_screen_default_at(key_path, …)`, sem overlay: recebe `&dyn Fn(HotkeyConfig)`. O valor inicial `Some(1)` volta a ser `Some(1)` depois de ligar e desligar; o inicial `None` volta a `None`.
- [ ] **Step 2: Rodar e ver falhar.** `cargo test -p nahshot`
- [ ] **Step 3: Implementar.** UI ainda não existe: apontar `frontendDist` para `../ui/dist` e criar um `ui/gallery.html` placeholder só pra compilar (a Task 11 substitui).
- [ ] **Step 4: Rodar e ver passar.** `cargo test --workspace`
- [ ] **Step 5: Verificação manual.** `cargo tauri dev`:
  - ícone na bandeja;
  - "Capturar" abre o overlay;
  - Ctrl+C cola no Paint **e** o PNG aparece em `Pictures\nahshot\AAAA-MM`;
  - Esc com seleção também salva;
  - "Salvar como…" grava a cópia escolhida;
  - fechar a janela placeholder não encerra o app;
  - abrir o exe duas vezes não cria segunda bandeja;
  - com `prints_dir` apontando para um drive inexistente (ex.: `Z:\x`), a captura cai em `unsaved\` com notificação.
- [ ] **Step 6: Commit.** `feat(app): tray, settings and capture flow`

---

### Task 11: Comandos da galeria + UI da galeria (layout A)

**Files:**
- Create: `src-tauri/src/commands.rs`
- Create: `ui/{package.json,vite.config.ts,tsconfig.json,gallery.html}`, `ui/src/{api.ts,group.ts,group.test.ts,gallery.ts,viewer.ts,theme.css}`
- Modify: `src-tauri/src/main.rs` (registrar comandos), `src-tauri/tauri.conf.json` (`beforeDevCommand: "npm --prefix ui run dev"`, `beforeBuildCommand: "npm --prefix ui run build"`, `devUrl: "http://localhost:5173"`)

**Interfaces:**
- Consumes: `Library`, `Query`, `Print`, `PrintId`, `ThumbCache` (core); `AppState` (Task 10)
- Produces, como comandos Tauri (todos `Result<T, String>`; o erro é a mensagem de `CoreError`):
  - `list_prints(query: Query) -> Vec<PrintDto>`, onde `PrintDto = Print + thumb: String` (path da miniatura; a UI usa `convertFileSrc`)
  - `get_sidebar() -> Sidebar { albums: Vec<String>, tags: Vec<String>, total: usize, favorites: usize }`
  - `rename_print(id, stem) -> PrintDto`, `set_favorite(id, value)`, `add_tag(id, tag)`, `remove_tag(id, tag)`
  - `create_album(name) -> String`, `rename_album(old, new) -> String`, `delete_album(name)`
  - `move_to_album(ids: Vec<PrintId>, album: String)`, `remove_from_album(ids)`
  - `delete_prints(ids)`, `copy_print(id)` (arboard), `reveal_print(id)` (`explorer.exe /select,"<path>"`)
  - `open_settings()`
  - Toda mutação emite `library-changed`.
- `ui/src/api.ts`: um wrapper tipado por comando, com os mesmos nomes em camelCase (`listPrints`, `getSidebar`…), e `onLibraryChanged(cb)`.
- `ui/src/group.ts`: `export function groupByDay(prints: {capturedAt: string}[], now: Date, order: "newest"|"oldest"): {label: string, items: typeof prints}[]`. Rótulos: "Hoje", "Ontem", senão `"4 de novembro de 2026"` (`Intl.DateTimeFormat("pt-BR", {day:"numeric", month:"long", year:"numeric"})`).

UI (layout A do mockup aprovado):
- **Topo:** busca com debounce de 200 ms e botão de ordem ("Mais novo ↓" / "Mais velho ↑").
- **Barra lateral:** "Todos (N)", "Favoritos (N)", seção Álbuns com "+ Novo álbum" (input inline; Enter cria; erro aparece inline em vermelho), seção Etiquetas. Clique direito num álbum → Renomear / Apagar. Apagar pede confirmação: "Os prints voltam para as pastas de mês. Nada é apagado."
- **Grade:**
  - miniaturas com `loading="lazy"`, agrupadas por `groupByDay`;
  - Ctrl+clique alterna a seleção; Shift+clique seleciona um intervalo;
  - arrastar a seleção até um álbum chama `moveToAlbum` (HTML5 drag-and-drop);
  - Delete → `deletePrints` com confirmação "Mover N print(s) para a Lixeira?".
- **Viewer** (duplo clique ou Enter):
  - overlay com a imagem grande via `convertFileSrc(path)`;
  - ações: Copiar, Abrir na pasta, Renomear, ★ Favorito, Álbum ▾ (lista + "Sem álbum"), Etiquetas (chips com × e input pra adicionar), Apagar;
  - Esc fecha; ←/→ navegam.
- `library-changed` → recarrega a lista e a barra lateral, mantendo filtro e scroll.
- Tema: variáveis CSS em `:root`, com `[data-theme="dark"]` e `prefers-color-scheme`. O app passa o tema via query string `?theme=`.

- [ ] **Step 1: Teste de `groupByDay` que falha** (`ui/src/group.test.ts`, Vitest)

```ts
const now = new Date("2026-11-04T15:00:00");
const ps = [{capturedAt:"2026-11-04T09:00:00"},{capturedAt:"2026-11-03T23:59:00"},{capturedAt:"2026-10-01T10:00:00"},{capturedAt:"2026-11-04T14:00:00"}];
test("labels and newest order", () => {
  const g = groupByDay(ps, now, "newest");
  expect(g.map(x => x.label)).toEqual(["Hoje","Ontem","1 de outubro de 2026"]);
  expect(g[0].items.map(i => i.capturedAt)).toEqual(["2026-11-04T14:00:00","2026-11-04T09:00:00"]);
});
test("oldest order reverses groups and items", () => {
  expect(groupByDay(ps, now, "oldest").map(x => x.label)).toEqual(["1 de outubro de 2026","Ontem","Hoje"]);
});
```

- [ ] **Step 2: Rodar e ver falhar.** `npm --prefix ui install && npm --prefix ui test` → FAIL.
- [ ] **Step 3: Implementar `group.ts`; rodar e ver passar.**
- [ ] **Step 4: Teste Rust de serialização.** Em `commands.rs`: `query_deserializes_from_ui_shape`. `{"filter":{"kind":"album","value":"Bugs"},"search":null,"order":"newestFirst"}` → `Filter::Album("Bugs")`, `SortOrder::NewestFirst`. Rodar `cargo test -p nahshot`, ver falhar, implementar `commands.rs`, ver passar.
- [ ] **Step 5: Implementar a UI** (`gallery.html`, `gallery.ts`, `viewer.ts`, `api.ts`, `theme.css`).
- [ ] **Step 6: Verificação manual.** `cargo tauri dev` → abrir galeria e conferir:
  - prints agrupados por dia;
  - inverter a ordem;
  - buscar por nome, etiqueta e álbum;
  - criar álbum `a/b` mostra erro;
  - criar "Bugs", arrastar 2 prints pra ele e checar no Explorer que estão em `Álbuns\Bugs`;
  - renomear o álbum e ver a pasta renomeada;
  - apagar o álbum e ver os prints de volta no mês;
  - renomear print e ver o arquivo renomeado;
  - favoritar;
  - pôr etiqueta;
  - apagar print e ver que foi pra Lixeira;
  - criar um PNG à mão na pasta pelo Explorer e ver que aparece sem recarregar;
  - tema escuro e claro;
  - fechar a galeria e ver que o app continua na bandeja.
- [ ] **Step 7: Commit.** `feat(gallery): gallery commands and layout A UI`

---

### Task 12: Configurações + boas-vindas

**Files:**
- Create: `ui/settings.html`, `ui/welcome.html`, `ui/src/{settings.ts,welcome.ts}`
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `Settings`, `apply_print_screen_default`, `Library`, `watch` (Task 10, core)
- Produces, como comandos:
  - `get_settings() -> Settings`
  - `set_autostart(bool)`, `set_default_print_screen(bool)` (usa `apply_print_screen_default`), `set_alt_hotkey(String)` (valida com `parse_accelerator`; erro → mensagem), `set_theme(Theme)`
  - `pick_prints_dir() -> Option<PathBuf>` (dialog de pasta)
  - `change_prints_dir(new: PathBuf, move_existing: bool) -> ChangeDirReport { moved: usize, failed: usize }`
  - `finish_welcome(enable_default: bool)`: grava `first_run_done = true` e aplica o Print Screen padrão se `true`
  - Todo setter salva as settings imediatamente.
- Core, novo: `impl Library { pub fn relocate(&mut self, new_root: PathBuf, move_existing: bool) -> Result<RelocateReport> }`, com `RelocateReport { moved: usize, failed: Vec<PathBuf> }`.
  - `move_existing = true`: move cada print **que está sob a raiz antiga** para o caminho relativo equivalente sob a nova (preservando `AAAA-MM\` e `Álbuns\nome\`), atualiza o path e conta falhas sem abortar.
  - Depois troca `root`.
  - Prints fora da raiz antiga não são tocados.
  - O app reinicia o watcher na nova raiz.

- [ ] **Step 1: Testes do core que falham** (`crates/nahshot-core/tests/library.rs`)
  - `relocate_with_move_preserves_structure`: um print no mês e um no álbum "Bugs" → os dois existem sob a nova raiz nas subpastas equivalentes, e `albums()` contém "Bugs".
  - `relocate_without_move_keeps_old_prints_visible`: `query` ainda retorna o print antigo e `sync()` não o remove.
  - `relocate_counts_failures_without_aborting`: um arquivo travado por um handle aberto conta como falha e os outros movem.
- [ ] **Step 2: Rodar e ver falhar; implementar `relocate`; ver passar.** `cargo test -p nahshot-core`
- [ ] **Step 3: Implementar comandos e UI.**
  - **Configurações:**
    - pasta atual + botão "Alterar…". Depois de escolher, o diálogo pergunta "Mover os prints existentes para a nova pasta?" com Sim, Não e Cancelar;
    - checkbox "Iniciar com o Windows";
    - checkbox "Usar o nahshot como Print Screen padrão";
    - campo de atalho que captura a combinação pressionada e mostra "Ctrl+Shift+S";
    - tema: Sistema, Claro ou Escuro.
  - **Boas-vindas:** título "Bem-vindo ao nahshot". Texto: "Quer usar o nahshot quando apertar Print Screen? Você pode mudar isso depois nas Configurações." Botões "Sim, usar o nahshot" e "Agora não".
- [ ] **Step 4: Verificação manual.** `cargo tauri dev` e conferir:
  - apagar `settings.json` → boas-vindas aparece;
  - "Sim" → Print Screen abre o overlay e `reg query "HKCU\Control Panel\Keyboard" /v PrintScreenKeyForSnippingEnabled` mostra `0x0`;
  - desmarcar nas Configurações → valor volta ao anterior (ou some, se não existia) e Print Screen volta a ser do Windows;
  - trocar o atalho pra `Ctrl+Alt+P` funciona;
  - trocar a pasta com "Sim" move os arquivos; com "Não", os antigos continuam na galeria;
  - autostart aparece em Gerenciador de Tarefas → Inicializar.
- [ ] **Step 5: Commit.** `feat(settings): settings window, welcome screen and prints folder relocation`

---

### Task 13: Leveza medida + README

**Files:**
- Create: `scripts/measure-idle.ps1`, `README.md`
- Modify: o que a medição apontar

**Interfaces:** nenhuma nova.

- [ ] **Step 1: Escrever `scripts/measure-idle.ps1`.**
  - Acha o processo `nahshot` e todos os descendentes via `Get-CimInstance Win32_Process` (seguindo `ParentProcessId` recursivamente).
  - Soma `PrivatePageCount` e `WorkingSetSize`.
  - Imprime `nahshot idle: private=XX.X MB, working_set=XX.X MB, processes=N`.
  - Retorna exit 1 se `private > 30 MB`.
- [ ] **Step 2: Medir.** Build release (`cargo tauri build`) e instalar. Medir nos três estados abaixo; o critério do spec é o idle.
  - (a) 60 s após o boot;
  - (b) depois de 5 capturas;
  - (c) depois de abrir e fechar a galeria e esperar 10 s.
- [ ] **Step 3: Corrigir o que estourar.** Suspeitos, em ordem:
  1. WebView2 continua vivo depois de fechar a galeria. Garantir que a janela foi destruída; se os processos `msedgewebview2` persistirem, os testes verão que eles somem em até ~10 s, porque o browser process do WebView2 encerra quando não sobra controller. Se não sumirem, investigar com systematic-debugging antes de mudar a arquitetura.
  2. Buffers do overlay não liberados.
  3. Alocador: testar `mimalloc` só se 1 e 2 estiverem ok.
  Registrar os números finais no README.
- [ ] **Step 4: Medir latência.** 10 capturas com `RUST_LOG=nahshot_capture=info`; a mediana de `overlay_visible_ms` tem que ser < 150. Se não for, perfilar `grab_all` (tempo por monitor) antes de mexer em outra coisa.
- [ ] **Step 5: README.md** (portfólio):
  - o que é e o porquê (1 parágrafo);
  - GIF/screenshot (placeholder `docs/media/` se ainda não houver);
  - recursos;
  - arquitetura: os 3 crates e por que o overlay é nativo;
  - números de RAM e latência medidos;
  - como compilar (`npm --prefix ui install`, `cargo tauri build`);
  - estrutura de pastas dos prints;
  - backlog.
- [ ] **Step 6: Gate final.** `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace && npm --prefix ui test` → tudo verde.
- [ ] **Step 7: Commit.** `docs: readme with measured footprint; perf: <o que foi ajustado>`
