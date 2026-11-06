# Overlay: dicas nas ferramentas e tela inteira

## O quê

Duas melhorias no overlay de captura (`crates/nahshot-capture/src/overlay/`).

### Dicas (tooltips) na toolbar

Com o mouse parado sobre um botão da toolbar, aparece uma caixinha escura com o nome
dele e o atalho, quando existir:

| Botão | Dica |
|---|---|
| Caneta, Linha, Seta, Retângulo, Elipse, Marca-texto, Texto | o nome da ferramenta |
| Cores | `Cor: Vermelho`, `Cor: Laranja`, … |
| Desfazer | `Desfazer (Ctrl+Z)` |
| Copiar | `Copiar (Ctrl+C)` |
| Salvar | `Salvar como (Ctrl+S)` |
| Fechar | `Fechar (Esc)` |

A dica aparece logo abaixo do botão; se não couber no monitor, logo acima. Fica sempre
dentro do monitor. Some ao tirar o mouse do botão ou ao começar qualquer arraste.

### Selecionar o monitor inteiro

Com o overlay aberto:

- **duplo-clique** numa área sem seleção, ou
- tecla **F** (fora da digitação de texto)

seleciona o monitor inteiro onde está o mouse. Depois disso tudo funciona igual: a
toolbar aparece, dá para desenhar, ajustar, copiar e salvar.

- Duplo-clique só vale onde um clique começaria uma seleção nova (não na toolbar, não
  sobre a seleção atual, não com desenhos na tela). Usa o tempo de duplo-clique do
  Windows e uma folga de 4 px entre os cliques.
- **F** com desenhos já feitos troca a seleção mantendo os desenhos no mesmo lugar da
  tela, como ao mover/redimensionar.

Para o recurso ser descoberto, enquanto não há seleção aparece no topo do monitor do
mouse a dica: `Arraste para selecionar · Duplo-clique ou F: tela inteira`.

## Como

- `toolbar.rs`: `ToolbarItem::tooltip()` dá o texto; `tooltip_layout()` posiciona a caixa.
- `session.rs`: guarda o botão sob o mouse (`hover`) e a dica já posicionada; o dano de
  repintura inclui dica e aviso. `select_monitor()` faz a seleção do monitor;
  `pressed_at(now)` detecta o duplo-clique (testável sem relógio real).
- `paint.rs`: desenha a dica e o aviso com o mesmo estilo do rótulo de tamanho.
- `mod.rs`: passa o tempo de duplo-clique do Windows (`GetDoubleClickTime`) para a sessão.

Sem fonte do sistema (caso raro), nem dica nem aviso aparecem; o resto funciona.
