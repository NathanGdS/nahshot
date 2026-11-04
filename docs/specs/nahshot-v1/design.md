# nahshot v1 — Design

Data: 2026-11-04
Status: rascunho, aguardando aprovação (revisado)

## 1. Intenção e porquê

nahshot é um app de captura de tela para Windows, no estilo do Lightshot: você aperta Print Screen, seleciona uma área, anota (setas, retângulos, texto…) e copia/salva.

A diferença: **todo print vai automaticamente para uma galeria organizada**. Hoje, com Lightshot/Ferramenta de Captura, o print é copiado e some, ou é salvo num lugar qualquer e depois ninguém lembra onde. O nahshot resolve isso: nenhum print se perde, e dá pra navegar, buscar e organizar (álbuns, etiquetas, favoritos) quando quiser. Organizar é opcional; sem mexer em nada, tudo continua acessível por data.

Motivação secundária: projeto de portfólio em Rust no GitHub do autor (que trabalha com Rust). O código deve ser limpo e idiomático, de um jeito que um dev Rust tenha orgulho de mostrar.

## 2. Objetivos

1. Captura tão rápida e simples quanto a do Lightshot, acionada pela tecla Print Screen.
2. Anotações básicas sobre a área selecionada.
3. Salvamento automático de toda captura confirmada numa pasta padrão.
4. Galeria para navegar (mais novo → mais velho por padrão), buscar e organizar com álbuns, etiquetas e favoritos.
5. Configurar o nahshot como handler padrão do Print Screen com um clique.
6. **Leve**: baixo consumo parado na bandeja, captura instantânea.

## 3. Não-objetivos (v1)

- Upload/nuvem/link compartilhável (backlog).
- Editar/reanotar um print antigo a partir da galeria (backlog).
- Gravação de vídeo/GIF, OCR, captura com rolagem, outros sistemas operacionais.

## 4. Anti-objetivos (o que tornaria isso um fracasso mesmo funcionando)

- **App pesado.** Se consumir muita RAM/CPU parado, ou se a captura demorar a aparecer, o usuário volta pro Lightshot. Este é o critério número 1.
- Perder um print. Se o usuário selecionou e copiou, ele precisa estar na galeria.
- Prender os prints num formato proprietário: os arquivos têm que ser PNGs normais, acessíveis pelo Explorer mesmo sem o app.

## 5. Restrições

- **Plataforma:** Windows 10 e 11 (64-bit). O autor usa Windows 10 Pro.
- **Stack (decisão do usuário):** Rust + Tauri (v2). Frontend do Tauri usado para a galeria e as configurações.
- 100% local, sem rede.
- Sem exigir privilégios de administrador.

## 6. Comportamento decidido

### 6.1 Segundo plano
- O app roda minimizado na bandeja do sistema. Menu da bandeja: Capturar, Abrir galeria, Configurações, Sair.
- Inicia com o Windows (padrão: ligado; configurável).
- Uma única instância por usuário.

### 6.2 Captura
- **Gatilho:** tecla Print Screen (configurável nas Configurações), além do menu da bandeja.
- A tela é congelada e escurecida imediatamente, cobrindo **todos os monitores**.
- *(Chamada do designer:)* a seleção pode atravessar monitores (como no Lightshot), e monitores com escalas de DPI diferentes precisam funcionar na v1. A imagem salva fica em pixels físicos.
- O usuário arrasta para selecionar uma região; o tamanho em pixels é exibido. A seleção pode ser ajustada (mover/redimensionar) antes de confirmar.
- Uma barra de ferramentas aparece colada à seleção com:
  - Ferramentas: caneta livre, linha, seta, retângulo, círculo/elipse, marca-texto, texto.
  - Seletor de cor. Desfazer (Ctrl+Z).
  - Ações: **Copiar** (Ctrl+C), **Salvar como…** (Ctrl+S, salva cópia em local escolhido além da galeria), **Fechar** (Esc).
- **Regra de salvamento:** qualquer captura com área selecionada é salva na galeria **sempre**, inclusive quando o usuário só copia, ou fecha com Esc depois de selecionar. *(Chamada do designer: Esc sem nenhuma área selecionada cancela sem salvar.)*
- A imagem salva é a área selecionada **com as anotações aplicadas** (achatada).

### 6.3 Armazenamento
- Imagens em PNG numa pasta normal do Windows. Padrão: `%USERPROFILE%\Pictures\nahshot\AAAA-MM\`, configurável.
- Nome de arquivo padrão baseado em data/hora (ex.: `2026-11-04_14-32-10.png`), renomeável na galeria.
- Metadados (etiquetas, favoritos, data de captura) num banco SQLite local no diretório de dados do app.
- *(Chamada do designer:)* desinstalar o app não apaga os PNGs.
- *(Chamada do designer:)* miniaturas geradas uma vez e guardadas em cache, para a galeria abrir rápido com milhares de prints.
- *(Chamada do designer:)* **fonte da verdade:** a pasta diz *quais* prints existem; o SQLite guarda só metadados. O app observa a pasta (file watcher + varredura ao abrir a galeria): PNG novo adicionado pelo Explorer aparece na galeria; PNG apagado/movido por fora some da galeria (metadados órfãos são limpos).
- *(Chamada do designer:)* renomear na galeria **renomeia o arquivo no disco**; não existe "nome exibido" separado.
- *(Chamada do designer:)* trocar a pasta dos prints nas Configurações não move nada automaticamente: oferece "mover prints existentes para a nova pasta?" (sim/não). Se não, os antigos continuam na galeria a partir do local original (o banco guarda caminhos absolutos).

### 6.4 Galeria (layout "A", aprovado no mockup)
- Janela com **barra lateral** à esquerda e **área principal** à direita; campo de busca e botão de ordenação no topo.
- Barra lateral: Todos, Favoritos, seção **Álbuns** (criar, renomear, apagar; botão "+ Novo álbum"), seção **Etiquetas**.
- Área principal: grade de miniaturas **agrupada por dia** (Hoje, Ontem, datas), do mais novo pro mais velho por padrão, com opção de inverter.
- Busca por nome do print, etiqueta ou álbum.
- Um print pode estar em um álbum e ter várias etiquetas.
- **Álbum = pasta de verdade no disco (decisão do usuário).** Cada álbum é uma subpasta em `<pasta dos prints>\Álbuns\<nome>\`. Por isso um print fica em no máximo um álbum. Etiquetas e favoritos ficam só no banco.
  - Mover um print para um álbum move o PNG para a subpasta; tirar do álbum devolve o PNG para a pasta do mês.
  - Criar/renomear um álbum cria/renomeia a pasta. Uma subpasta criada pelo Explorer dentro de `Álbuns\` aparece como álbum.
  - *(Chamada do designer:)* apagar um álbum devolve os prints dele para as pastas de mês e só depois remove a pasta vazia. Nunca apaga prints.
  - *(Chamada do designer:)* o agrupamento por dia usa a data de captura (guardada no banco e no nome do arquivo), não a pasta onde o PNG está.
- Abrir um print mostra a imagem grande. Ações: copiar, abrir na pasta (Explorer), renomear, favoritar, mover para álbum, adicionar/remover etiqueta, apagar.
- *(Chamada do designer:)* seleção múltipla (Ctrl/Shift+clique) e arrastar para um álbum na barra lateral.
- *(Chamada do designer:)* apagar manda o PNG para a Lixeira do Windows.
- A janela da galeria só existe enquanto está aberta; fechar a galeria não encerra o app (volta pra bandeja).

### 6.5 Configurações
- Pasta dos prints.
- Iniciar com o Windows.
- **"Usar o nahshot como Print Screen padrão"**: desativa a opção do Windows que faz o Print Screen abrir a Ferramenta de Captura (chave de registro por usuário, `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled`) e passa a interceptar a tecla. Desmarcar restaura o estado anterior.
  - *(Chamada do designer:)* ligado → hook de teclado de baixo nível que intercepta o Print Screen e **bloqueia** a ação do Windows (nem Ferramenta de Captura, nem cópia da tela inteira). Desligado → o Print Screen é do Windows; o nahshot fica acessível pelo menu da bandeja e por um atalho alternativo (padrão `Ctrl+Shift+S`, configurável), registrado como hotkey normal.
- Tecla de atalho de captura.
- Tema claro/escuro.
- Primeira execução: uma tela de boas-vindas que oferece ativar o nahshot como Print Screen padrão.

## 7. Metas de leveza (chamada do designer, para validar)

- RAM parado na bandeja, sem a galeria aberta: alvo < ~30 MB, contando o processo do nahshot **e** qualquer processo filho (WebView2). Ou seja, parado não pode haver webview viva.
- Abrir a galeria a frio (subir o WebView2) em ~1 s é aceitável; é o preço de não ocupar memória parado.
- Do Print Screen até o overlay de seleção visível: alvo < ~150 ms.
- Implicação: o overlay de captura **não deve depender de inicializar um webview a cada captura**. Fica a cargo do builder escolher entre um overlay nativo (ex.: Win32/Direct2D, `softbuffer`/`tiny-skia`) ou um webview pré-aquecido, desde que as metas sejam atingidas e medidas.

## 8. Deixado para o builder

- Arquitetura interna (crates, módulos, organização do workspace).
- Biblioteca de captura de tela e de hook de teclado.
- Framework do frontend da galeria (ou HTML/TS puro), desde que leve.
- Esquema do SQLite e estratégia de migração.
- Estilo visual, desde que siga o layout A e tenha tema claro/escuro.
- Estratégia de testes.

## 9. Backlog (pós-v1)

- Editar/reanotar print antigo a partir da galeria.
- Upload para nuvem com link compartilhável.
