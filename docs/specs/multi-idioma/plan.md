# Plano: multi-idioma

Design: [design.md](design.md).

1. **Locales + crate `nahshot-i18n`**
   - `locales/{en,pt,es}.json` com todas as chaves.
   - `crates/nahshot-i18n`: `Lang`, `set_lang`/`lang`, `t`/`tf`, `system_lang`, testes.
2. **UI web**
   - `ui/src/i18n.ts` (+ teste de chaves/placeholders/uso no código).
   - `vite.config.ts` (`server.fs.allow`), `tsconfig.json` (`resolveJsonModule`).
   - HTMLs com `data-i18n*`; `api.ts`, `ui.ts`, `gallery.ts`, `sidebar.ts`,
     `viewer.ts`, `settings.ts`, `welcome.ts`, `group.ts` usando `t`/`tn`.
   - Seletor de idioma em Configurações.
3. **Rust nativo**
   - `Settings.language` + `set_language`; idioma aplicado no setup.
   - `windows.rs` (títulos, `?lang=`, recarregar janelas), `tray.rs` (reconstruir
     menu), `updater.rs`, `capture_flow.rs`.
   - `nahshot-capture`: toolbar, dica do overlay, toast.
4. **Pasta `Albums`**
   - `naming.rs`, `library.rs` (`albums_dir` ativo), `migrate_albums_dir`,
     `relocate.rs`, ajustes em testes; novos testes de migração.
5. **Verificação**
   - `cargo fmt`, `clippy -D warnings`, `cargo test --workspace`, `npm run build`,
     `npm test`.
   - Rodar o app: os três idiomas, trocar com as janelas abertas, migrar uma
     biblioteca real com `Álbuns`.
   - PR.
