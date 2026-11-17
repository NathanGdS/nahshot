# Plano: seleção múltipla

1. **Core** (`crates/nahshot-core`): `Library::set_favorite` e `Library::add_tag` recebem
   `&[PrintId]` e rodam em `Db::transaction`; se um id não existe, nada muda. Testes em
   `tests/library.rs`.
2. **Comandos** (`src-tauri/src/commands.rs`): `set_favorite { ids, value }` e
   `add_tag { ids, tag }`. `ui/src/api.ts` e o visualizador passam `[id]`.
3. **Lógica pura** (`ui/src/selection.ts`, testada com vitest): clique (simples, Ctrl,
   Shift, check, modo seleção), estado do check do dia, regra do Favoritar, interseção do
   retângulo com os tiles.
4. **Galeria** (`ui/src/gallery.ts`, `ui/gallery.html`, `ui/src/theme.css`): check
   clicável, modo seleção, barra de seleção, menu do clique direito, check do dia,
   retângulo com auto-scroll.
5. **Verificação**: `cargo test --workspace`, `npm run build`, `npm test`, `cargo clippy`,
   e teste manual no app (`make dev`) de cada item do design.
