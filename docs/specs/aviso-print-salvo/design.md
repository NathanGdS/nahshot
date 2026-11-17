# Aviso de "print salvo"

## Por quê

Todo print finalizado vai pra galeria automaticamente, mas nada na tela diz isso. Quem
copia com Ctrl+C ou fecha com Esc (com uma seleção feita, o Esc **também salva**) pode
nem saber que o print ficou guardado. O aviso lembra, de leve, que o print está na
galeria.

## O quê

Um cartãozinho desenhado pelo próprio nahshot (estilo 3 dos mockups, "cartão com
contador") aparece sempre que um print é salvo na galeria:

```
                              ┌──────────────────────────────────┐
                              │ (✓)  Print salvo                 │
                              │      Está na galeria do nahshot  │
                              │▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔                 │  ← barra azul que esvazia
                              └──────────────────────────────────┘
──────────────────────────────────────────── barra de tarefas ────
```

- **Onde:** canto inferior direito da área útil (acima da barra de tarefas) do monitor
  onde está o cursor, que é onde o print acabou de ser tirado.
- **Visual:** cartão com cantos arredondados (~10 px), sombra suave, borda de 1 px, check
  azul num círculo de azul clarinho, título em negrito, subtítulo cinza, barra azul de
  3 px no rodapé que esvazia da direita pra esquerda enquanto o tempo passa. Cores da
  paleta do app (`ui/src/theme.css`).
- **Tema:** segue a configuração de tema do app (Claro / Escuro / Sistema).
- **Animação:** entra deslizando da direita com fade (~250 ms), fica ~2,3 s, sai com fade
  (~300 ms) e a janela é destruída. Escalado por DPI do monitor.
- **Comportamento:** sem som, não rouba o foco, não aparece na barra de tarefas, clique
  passa através (não bloqueia o que está embaixo). Um print novo enquanto o aviso está
  na tela substitui o aviso e reinicia o tempo (nunca empilha).
- **Texto por ação** (título sempre "Print salvo"):
  - Ctrl+C / Copiar → "Copiado · está na galeria"
  - Esc com seleção → "Está na galeria do nahshot"
  - Salvar como → depois que a janela de salvar fecha: "Está na galeria do nahshot"
    (salvou a cópia ou cancelou, o print está na galeria de qualquer jeito)
- **Quando aparece:** o print foi gravado na pasta dos prints (registrado na hora ou
  só no disco, esperando o sync). Se a gravação falhar, continua a notificação de erro
  atual do Windows e não aparece o cartão.

### Fora do escopo

- Clicar no aviso não faz nada (opção A: só texto). Abrir a galeria pelo aviso fica pra
  depois, se quiser.
- Sem opção de desligar nas Configurações por enquanto.
- Ações dentro da galeria (copiar, apagar, etc.) não mostram o aviso: ele é só pra
  captura nova.

### Anti-objetivos

- Pesar parado: nada pode ficar vivo entre um aviso e outro (meta de RAM parado < 30 MB).
- Atrasar a captura: o aviso não pode segurar o overlay nem a cópia pra área de
  transferência.
- Atrapalhar: roubar foco, tocar som, ficar na frente de clique.

## Abordagens

1. **Janela nativa em camadas (Win32 layered window), desenhada com tiny-skia.**
   *Escolhida.* O crate `nahshot-capture` já desenha com tiny-skia e fontdue (Segoe UI).
   Janela `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE |
   WS_EX_TRANSPARENT`, pintada com `UpdateLayeredWindow` (alpha por pixel → cantos
   redondos e sombra de verdade). Abre na hora, custa quase nada, some por completo.
2. **Janela Tauri/webview com HTML.** Visual mais fácil de ajustar, mas sobe o WebView2
   a cada aviso (~1 s a frio e dezenas de MB). Bate de frente com o critério número 1 do
   app. Descartada.
3. **Notificação do Windows.** Descartada pelo usuário.

## O que fica pro construtor

- Em que thread a janela vive (provavelmente a do overlay, que já tem loop de mensagens
  Win32 pelo winit) e como o tempo da animação é dirigido (timer ou `WaitUntil` do winit).
- Como o tema chega no crate de captura (parâmetro na chamada, sem o crate conhecer as
  configurações do Tauri).
- Medidas exatas (largura ~260 px lógicos), tamanho de fonte, desenho do check.
- Testes: lógica pura (texto por ação, posição no monitor, curva da animação, substituir
  aviso) com testes unitários; o desenho conferido rodando o app.
