# Aviso de "print salvo" — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mostrar um cartão nativo "Print salvo" (com barra de tempo) no canto inferior
direito do monitor do cursor sempre que uma captura nova é gravada na galeria.

**Architecture:** Novo módulo `toast` no crate `nahshot-capture`: lógica pura (texto,
cores, layout, linha do tempo da animação) → pintura com tiny-skia num `Pixmap` →
janela Win32 em camadas (`UpdateLayeredWindow`) numa thread própria e curta, que morre
quando o aviso some. O app (`src-tauri/src/capture_flow.rs`) chama
`toast::show(kind, dark)` depois de gravar o print.

**Tech Stack:** Rust, windows-sys 0.61, tiny-skia 0.12, fontdue 0.9, Tauri v2.

**Spec:** `docs/specs/aviso-print-salvo/design.md`

## Global Constraints

- Título sempre `Print salvo`. Subtítulos exatos: Ctrl+C → `Copiado · está na galeria`;
  Esc com seleção e Salvar como → `Está na galeria do nahshot`.
- Tempos: entrada 250 ms (desliza 24 px lógicos da direita + fade), fica 2300 ms, saída
  300 ms (fade). Barra esvazia linearmente durante os 2300 ms.
- Medidas lógicas (×escala DPI do monitor): cartão 260×64, raio 10, borda 1, margem até
  a borda da área útil 16, barra 3 px no rodapé, círculo do check 28, título 14 px
  negrito, subtítulo 12 px.
- Cores (de `ui/src/theme.css`):
  - claro: fundo `#ffffff`, borda `#e3e4e8`, título `#1a1b1e`, subtítulo `#5d6069`,
    destaque `#4a6cf7`, círculo = destaque a 14 %.
  - escuro: fundo `#2a2b30`, borda `#3a3c43`, título `#ececef`, subtítulo `#a4a7b0`,
    destaque `#6e8bff`, círculo = destaque a 20 %.
- Sombra: suave, ~16 px lógicos de desfoque, preto a ~18 % (claro) / ~45 % (escuro);
  a janela tem margem transparente pra caber a sombra.
- Janela: `WS_POPUP`, `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW |
  WS_EX_NOACTIVATE | WS_EX_TRANSPARENT`; mostrada com `SW_SHOWNOACTIVATE`. Sem som.
- Nada fica vivo depois que o aviso some (meta de RAM parado < 30 MB). Nunca criar o
  aviso na thread do overlay.
- Um aviso novo fecha o anterior; nunca dois na tela.
- Gravação recusada (`Imported::Refused`) → só a notificação de erro atual, sem cartão.
- Commits sem `Co-Authored-By`.

## Review Focus

1. **Dois prints seguidos (< 3 s):** o segundo substitui o primeiro; nunca duas janelas,
   nenhuma thread presa. → teste em Task 3 (`replacing_closes_previous`) + verificação
   manual em Task 5.
2. **Monitores com DPI diferente / barra de tarefas à esquerda ou em cima:** o cartão
   fica dentro da área útil do monitor do cursor, na escala certa. → testes de layout em
   Task 1 (`layout_respects_work_area_offset`, `layout_scales`).
3. **Foco:** depois do Ctrl+C, Ctrl+V no app de onde veio funciona e a digitação não é
   interrompida. → verificação manual em Task 5.
4. **Salvar como cancelado:** o cartão ainda aparece (o print está na galeria). → teste
   em Task 4 (`toast_kind_for_actions`) + manual.
5. **Tema Sistema com Windows escuro:** o cartão sai escuro. → teste em Task 4
   (`resolve_dark`).

---

## File Structure

- Create `crates/nahshot-capture/src/toast/mod.rs` — API pública `ToastKind`, `show`.
- Create `crates/nahshot-capture/src/toast/model.rs` — texto, paleta, layout, timeline (puro).
- Create `crates/nahshot-capture/src/toast/paint.rs` — desenha um quadro num `Pixmap`.
- Create `crates/nahshot-capture/src/toast/window.rs` — janela em camadas + thread.
- Create `crates/nahshot-capture/examples/toast.rs` — mostra os avisos pra conferir à mão.
- Modify `crates/nahshot-capture/src/lib.rs` — `pub mod toast;`.
- Modify `crates/nahshot-capture/src/font.rs` — `Font::system_bold()`.
- Modify `crates/nahshot-capture/src/registry.rs` — `apps_use_dark_theme()`.
- Modify `crates/nahshot-capture/Cargo.toml` — features `Win32_Graphics_Gdi`, `Win32_UI_HiDpi`.
- Modify `src-tauri/src/capture_flow.rs` — chama o aviso.

---

### Task 1: Modelo puro do aviso

**Files:**
- Create: `crates/nahshot-capture/src/toast/model.rs`, `crates/nahshot-capture/src/toast/mod.rs`
- Modify: `crates/nahshot-capture/src/lib.rs`

**Interfaces:**
- Produces (em `toast::model`, re-exportado `ToastKind` em `toast`):
  - `pub enum ToastKind { Copied, Saved }` (`Debug, Clone, Copy, PartialEq, Eq`)
  - `impl ToastKind { pub fn title(self) -> &'static str; pub fn subtitle(self) -> &'static str }`
  - `pub struct Palette { pub card, border, title, subtitle, accent, check_bg, shadow: tiny_skia::Color }` + `pub fn palette(dark: bool) -> Palette`
  - `pub const CARD_W: f32 = 260.0; CARD_H: f32 = 64.0; MARGIN: f32 = 16.0; SHADOW: f32 = 16.0; SLIDE: f32 = 24.0;`
  - `pub fn layout(work: PxRect, scale: f32) -> PxRect` — retângulo da **janela** (cartão + `SHADOW*scale` de cada lado), cartão encostado a `MARGIN*scale` da direita e de baixo de `work`.
  - `pub const ENTER: Duration = 250ms; HOLD: Duration = 2300ms; EXIT: Duration = 300ms;`
  - `pub struct Frame { pub opacity: f32, pub offset_x: f32, pub bar: f32 }` (offset em px lógicos, bar 0..=1)
  - `pub fn frame_at(elapsed: Duration) -> Option<Frame>` — `None` depois de `ENTER+HOLD+EXIT`.

- [ ] **Step 1: Testes que falham** em `model.rs` (`#[cfg(test)]`):

```rust
#[test] fn texts() {
    assert_eq!(ToastKind::Copied.title(), "Print salvo");
    assert_eq!(ToastKind::Saved.title(), "Print salvo");
    assert_eq!(ToastKind::Copied.subtitle(), "Copiado · está na galeria");
    assert_eq!(ToastKind::Saved.subtitle(), "Está na galeria do nahshot");
}
#[test] fn layout_bottom_right_at_100() {
    let w = layout(PxRect { x: 0, y: 0, w: 1920, h: 1040 }, 1.0);
    assert_eq!((w.w, w.h), (260 + 32, 64 + 32));
    assert_eq!(w.x + w.w as i32, 1920 - 16 + 16); // borda do cartão a 16 da direita; sombra passa
    assert_eq!(w.y + w.h as i32, 1040 - 16 + 16);
}
#[test] fn layout_scales() {
    let w = layout(PxRect { x: 0, y: 0, w: 2880, h: 1560 }, 1.5);
    assert_eq!((w.w, w.h), (438, 144));
}
#[test] fn layout_respects_work_area_offset() {
    // barra de tarefas à esquerda + segundo monitor à esquerda (x negativo)
    let work = PxRect { x: -1920 + 48, y: 0, w: 1920 - 48, h: 1080 };
    let w = layout(work, 1.0);
    assert!(w.x >= work.x && w.x + w.w as i32 <= work.x + work.w as i32 + 16);
    assert_eq!(w.x + w.w as i32, 0);
}
#[test] fn timeline() {
    let a = frame_at(Duration::ZERO).unwrap();
    assert_eq!((a.opacity, a.offset_x, a.bar), (0.0, SLIDE, 1.0));
    let b = frame_at(ENTER).unwrap();
    assert_eq!((b.opacity, b.offset_x, b.bar), (1.0, 0.0, 1.0));
    let c = frame_at(ENTER + HOLD / 2).unwrap();
    assert!((c.bar - 0.5).abs() < 0.01 && c.opacity == 1.0);
    let d = frame_at(ENTER + HOLD + EXIT / 2).unwrap();
    assert!(d.opacity > 0.0 && d.opacity < 1.0 && d.bar == 0.0);
    assert!(frame_at(ENTER + HOLD + EXIT).is_none());
}
#[test] fn palettes_differ() {
    assert_ne!(palette(false).card, palette(true).card);
}
```

- [ ] **Step 2: Rodar e ver falhar** — `cargo test -p nahshot-capture toast::model` → erro de compilação (módulo não existe).
- [ ] **Step 3: Implementar** `model.rs`, `mod.rs` (`mod model; pub use model::ToastKind;` — `paint`/`window` entram nas próximas tasks) e `pub mod toast;` em `lib.rs`. Entrada com easing ease-out cúbico (`1-(1-t)^3`) em opacidade e deslize; saída linear na opacidade.
- [ ] **Step 4: Rodar e ver passar** — `cargo test -p nahshot-capture toast::model` → todos PASS.
- [ ] **Step 5: Commit** — `feat(capture): toast model (texts, layout, timeline)`

---

### Task 2: Pintura do cartão

**Files:**
- Create: `crates/nahshot-capture/src/toast/paint.rs`
- Modify: `crates/nahshot-capture/src/font.rs`, `crates/nahshot-capture/src/toast/mod.rs`

**Interfaces:**
- Consumes: `ToastKind`, `Palette`, `palette`, constantes e `Frame` da Task 1; `render::draw_text` (já existe, `pub(crate)`).
- Produces:
  - `Font::system_bold() -> Result<Font, CaptureError>` — candidatos `segoeuib.ttf`, `arialbd.ttf`.
  - `pub(crate) struct Fonts { pub regular: Font, pub bold: Font }` + `Fonts::load() -> Result<Fonts, CaptureError>`.
  - `pub(crate) fn paint(kind: ToastKind, pal: &Palette, scale: f32, frame: &Frame, fonts: &Fonts) -> tiny_skia::Pixmap` — tamanho da janela de `layout` (sem o deslize: o deslize é aplicado movendo a janela); conteúdo já multiplicado por `frame.opacity`; pixels premultiplicados RGBA.

- [ ] **Step 1: Testes que falham** em `paint.rs`:

```rust
fn px(p: &Pixmap, x: u32, y: u32) -> [u8; 4] { let c = p.pixel(x, y).unwrap(); [c.red(), c.green(), c.blue(), c.alpha()] }
fn full() -> Frame { Frame { opacity: 1.0, offset_x: 0.0, bar: 1.0 } }

#[test] fn size_matches_layout() {
    let p = paint(ToastKind::Saved, &palette(false), 1.0, &full(), &Fonts::load().unwrap());
    assert_eq!((p.width(), p.height()), (292, 96));
}
#[test] fn corner_transparent_center_opaque() {
    let p = paint(ToastKind::Saved, &palette(false), 1.0, &full(), &Fonts::load().unwrap());
    assert!(px(&p, 0, 0)[3] < 8); // só resto de sombra, no máximo
    assert_eq!(px(&p, 16 + 200, 16 + 10)[3], 255);
}
#[test] fn bar_follows_fraction() {
    let f = Fonts::load().unwrap();
    let half = paint(ToastKind::Saved, &palette(false), 1.0, &Frame { bar: 0.5, ..full() }, &f);
    let y = 16 + 64 - 2; // dentro da barra
    let accent = palette(false).accent.to_color_u8();
    assert_eq!(&px(&half, 16 + 40, y)[..3], &[accent.red(), accent.green(), accent.blue()]);
    assert_ne!(&px(&half, 16 + 220, y)[..3], &[accent.red(), accent.green(), accent.blue()]);
}
#[test] fn opacity_scales_alpha() {
    let f = Fonts::load().unwrap();
    let p = paint(ToastKind::Saved, &palette(true), 1.0, &Frame { opacity: 0.5, ..full() }, &f);
    let a = px(&p, 16 + 200, 16 + 10)[3];
    assert!((120..=135).contains(&a));
}
#[test] fn bold_font_loads() { assert!(Font::system_bold().is_ok()); }
```

- [ ] **Step 2: Rodar e ver falhar** — `cargo test -p nahshot-capture toast::paint` → não compila.
- [ ] **Step 3: Implementar.** Ordem: sombra (retângulo arredondado preenchido com `pal.shadow`, desfocado com box blur em 3 passadas sobre o canal alfa, deslocado 4 px lógicos pra baixo), cartão (fill `card`, stroke 1 px `border`, raio 10), círculo do check (28 px, `check_bg`, centro a 14+12 px da esquerda, centro vertical) com o ✓ como path (stroke 2,5 px `accent`, pontas redondas), título bold 14 px e subtítulo 12 px a 54 px da esquerda, barra 3 px `accent` de largura `bar * CARD_W` recortada pelo cartão arredondado (clip mask). Por fim multiplica todos os bytes por `opacity`. Texto via `draw_text` sobre o `PixmapMut`.
- [ ] **Step 4: Rodar e ver passar** — `cargo test -p nahshot-capture toast` → PASS.
- [ ] **Step 5: Salvar PNG pra olhar** — teste `#[ignore] fn dump_png()` que grava `target/toast-light.png` e `target/toast-dark.png`; rodar `cargo test -p nahshot-capture dump_png -- --ignored` e abrir as imagens. Conferir contra o mockup (estilo 3).
- [ ] **Step 6: Commit** — `feat(capture): paint the saved-print toast`

---

### Task 3: Janela em camadas + `toast::show`

**Files:**
- Create: `crates/nahshot-capture/src/toast/window.rs`, `crates/nahshot-capture/examples/toast.rs`
- Modify: `crates/nahshot-capture/src/toast/mod.rs`, `crates/nahshot-capture/Cargo.toml`

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces: `pub fn show(kind: ToastKind, dark: bool)` em `toast` — não bloqueia, retorna na hora; falhas só vão pro log (`warn!`).

- [ ] **Step 1: Teste que falha** — a parte testável é o "substituir": um registro global do aviso atual.

```rust
// window.rs
#[test] fn replacing_closes_previous() {
    let slot = Current::default();
    let first = slot.claim();          // geração 1
    let second = slot.claim();         // geração 2
    assert!(!slot.is_current(first));
    assert!(slot.is_current(second));
}
```

`Current` = `AtomicU64` de geração + `Mutex<Option<isize>>` com o HWND ativo; `claim()` incrementa a geração, faz `PostMessageW(hwnd_antigo, WM_CLOSE)` se houver, e devolve a geração nova.

- [ ] **Step 2: Rodar e ver falhar** — `cargo test -p nahshot-capture toast::window` → não compila.
- [ ] **Step 3: Implementar `show`.** Spawna thread `nahshot-toast`; nela:
  1. `SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)`.
  2. `GetCursorPos` → `MonitorFromPoint(MONITOR_DEFAULTTONEAREST)` → `GetMonitorInfoW` (`rcWork`) → `GetDpiForMonitor(MDT_EFFECTIVE_DPI)`; `scale = dpi / 96`.
  3. `Fonts::load()` (falhou → `warn!` e sai), `layout(work, scale)`.
  4. Registra classe (uma vez, `OnceLock`) e cria a janela com os estilos das Global Constraints; guarda o HWND em `Current`.
  5. `SetTimer` de 16 ms; a cada `WM_TIMER`: `frame_at(start.elapsed())` → `None` ⇒ `DestroyWindow`; senão `paint(...)`, converte RGBA premultiplicado → BGRA num DIB section 32 bpp top-down e chama `UpdateLayeredWindow` com `BLENDFUNCTION { AC_SRC_OVER, 0, 255, AC_SRC_ALPHA }` na posição `x + offset_x*scale`.
  6. `WM_CLOSE` → `DestroyWindow`; `WM_DESTROY` → limpa o HWND em `Current` se ainda for o dela, `PostQuitMessage(0)`; o loop `GetMessageW` termina e a thread acaba (libera DIB/DC).
  Adicionar features `Win32_Graphics_Gdi` e `Win32_UI_HiDpi` ao windows-sys.
- [ ] **Step 4: Rodar e ver passar** — `cargo test -p nahshot-capture toast` → PASS.
- [ ] **Step 5: Exemplo manual** — `examples/toast.rs`: `show(Copied, false)`, dorme 1 s, `show(Saved, true)` (deve substituir), dorme 4 s, sai. Rodar `cargo run -p nahshot-capture --example toast` e conferir: aparece no canto do monitor do cursor, desliza, barra esvazia, some; o segundo substitui o primeiro; o foco do terminal não muda (dá pra continuar digitando); clique passa através.
- [ ] **Step 6: Commit** — `feat(capture): native layered toast window`

---

### Task 4: Ligar no app

**Files:**
- Modify: `crates/nahshot-capture/src/registry.rs`, `src-tauri/src/capture_flow.rs`

**Interfaces:**
- Consumes: `toast::{show, ToastKind}`; `Settings.theme: Theme { System, Light, Dark }` (`src-tauri/src/settings.rs`).
- Produces:
  - `pub fn apps_use_dark_theme() -> bool` em `registry` — lê `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` `AppsUseLightTheme`; `Some(0)` ⇒ `true`; ausente/erro ⇒ `false`. Generalizar `read_snipping` num `read_dword(key, value)` privado.
  - em `capture_flow.rs`: `fn toast_kind(action: Action) -> ToastKind` (Copy → Copied, SaveAs/Close → Saved) e `fn resolve_dark(theme: Theme, system_dark: bool) -> bool`.

- [ ] **Step 1: Testes que falham** em `capture_flow.rs`:

```rust
#[test] fn toast_kind_for_actions() {
    assert_eq!(toast_kind(Action::Copy), ToastKind::Copied);
    assert_eq!(toast_kind(Action::SaveAs), ToastKind::Saved);
    assert_eq!(toast_kind(Action::Close), ToastKind::Saved);
}
#[test] fn resolve_dark() {
    assert!(super::resolve_dark(Theme::Dark, false));
    assert!(!super::resolve_dark(Theme::Light, true));
    assert!(super::resolve_dark(Theme::System, true));
    assert!(!super::resolve_dark(Theme::System, false));
}
```

- [ ] **Step 2: Rodar e ver falhar** — `cargo test -p nahshot capture_flow` → não compila.
- [ ] **Step 3: Implementar.** Em `handle`: guardar se o import deu `Registered` ou `OnDisk` (`saved: bool`). Depois do bloco do `SaveAs` (ou seja, depois que o diálogo fecha, salvando ou cancelando), se `saved`: `toast::show(toast_kind(r.action), dark)`, com `dark` lido de `AppState.settings` (sem estado ⇒ `Theme::System`) e `registry::apps_use_dark_theme()`. `Refused` não mostra.
- [ ] **Step 4: Rodar e ver passar** — `cargo test --workspace` → tudo PASS; `cargo clippy --workspace --all-targets -- -D warnings` limpo.
- [ ] **Step 5: Commit** — `feat(app): show the saved-print toast after each capture`

---

### Task 5: Verificação no app de verdade

- [ ] **Step 1:** `make build` e abrir `target/release/nahshot.exe`.
- [ ] **Step 2:** Print + Ctrl+C → cartão "Copiado · está na galeria"; colar com Ctrl+V no Paint/chat funciona logo em seguida; o foco não sai do app anterior.
- [ ] **Step 3:** Print + Esc com seleção → "Está na galeria do nahshot". Print + Esc sem seleção → nada.
- [ ] **Step 4:** Salvar como → cartão aparece depois que o diálogo fecha, tanto salvando quanto cancelando.
- [ ] **Step 5:** Dois prints em menos de 3 s → só um cartão por vez.
- [ ] **Step 6:** Tema Claro, Escuro e Sistema (com Windows em modo escuro) nas Configurações → cores certas.
- [ ] **Step 7:** Se houver segundo monitor/DPI diferente, tirar print nele → cartão no canto dele.
- [ ] **Step 8:** Depois que o cartão some, `powershell -ExecutionPolicy Bypass -File scripts\measure-idle.ps1` → exit 0 (< 30 MB); e nenhuma thread `nahshot-toast` sobrando.
- [ ] **Step 9:** Abrir PR da branch `feat/aviso-print-salvo` pra `main`.
