# Galeria: seleção múltipla visível e ações em lote

## Por quê

A galeria já sabia selecionar vários prints (Ctrl+clique, Shift+clique, Ctrl+A, Delete,
arrastar pra um álbum), mas nada na tela mostrava isso:

- o círculo de check que aparece no hover **parecia** um botão de marcar, mas clicar nele
  era um clique comum no tile: selecionava só aquele e desmarcava o resto;
- com vários marcados, não aparecia nenhuma ação;
- o clique direito estava bloqueado e não abria menu.

Resultado: pra apagar vários prints o usuário abria um por um no visualizador.

## O quê

### 1. Check que marca de verdade

- Clicar no círculo do tile **alterna** aquele print na seleção, sem mexer nos outros.
- Com pelo menos um print marcado, a galeria fica em **modo seleção**: clique simples num
  tile também alterna (como Google Fotos). Os círculos ficam visíveis em todos os tiles.
- Ctrl+clique alterna, como antes. Shift+clique **soma** o intervalo desde o último print
  clicado (antes substituía a seleção; somar combina com o modo seleção e não perde o que
  já estava marcado). Clique simples sem nada marcado seleciona só aquele (como antes).
- Clique duplo continua abrindo o visualizador.
- Esc, o ✕ da barra ou clique em área vazia limpam a seleção.

### 2. Barra de seleção

Com algo marcado, a toolbar do topo vira a barra de seleção:

```
[✕] 5 selecionados   ★ Favoritar  📁 Mover p/ álbum  # Etiqueta  ⧉ Copiar  📂 Pasta  🗑 Apagar
```

- **Favoritar**: se algum marcado não é favorito, favorita todos; se todos já são,
  vira **Desfavoritar** e desfavorita todos.
- **Mover p/ álbum**: menu com os álbuns e "Sem álbum".
- **Etiqueta**: campo pra digitar uma etiqueta, aplicada a todos os marcados.
- **Copiar** e **Abrir na pasta**: só com exatamente 1 marcado (a área de transferência
  guarda uma imagem; o Explorer seleciona um arquivo).
- **Apagar**: confirma e manda todos pra Lixeira.
- **Selecionar tudo** (Ctrl+A) também fica como botão na barra.

### 3. Menu do clique direito

Clique direito num tile abre um menu com as mesmas ações (mais "Abrir" quando é um só).
Se o tile clicado não está marcado, ele passa a ser a seleção antes do menu abrir.
"Mover para álbum…" abre a lista de álbuns no mesmo lugar.

### 4. Marcar o dia inteiro

Ao lado do título de cada dia ("Hoje", "Ontem", …) há um círculo de check. Marcado quando
todos os prints do dia estão marcados; clicar marca o dia todo ou, se já estava todo
marcado, desmarca o dia.

### 5. Seleção por retângulo (arrastar)

- Apertar o botão esquerdo numa **área vazia** da galeria (fora de um tile) e arrastar
  desenha um retângulo; todo tile que encosta no retângulo fica marcado, em tempo real.
- Sem Ctrl/Shift, o retângulo substitui a seleção; com Ctrl ou Shift, soma à seleção
  que existia antes do arraste.
- Perto da borda de cima/baixo a lista rola sozinha, e o retângulo acompanha.
- Arrastar **a partir de um tile** continua sendo o arraste pra álbum (sem conflito).
- Um clique sem arrastar (menos de 4 px) em área vazia continua limpando a seleção.

### 6. Backend em lote

`set_favorite` e `add_tag` passam a receber uma lista de ids e rodam numa transação só
(um único `library-changed`). Mover e apagar já eram em lote.

## Fora do escopo

- Desfazer depois de apagar (restaurar da Lixeira).
- Dica de primeiro uso.
- Remover etiqueta em lote.

## Abordagens consideradas

- **Só documentar os atalhos** (Ctrl/Shift): barato, mas não resolve; ninguém descobre.
- **Checkbox + modo seleção + barra** (escolhida): padrão conhecido (Google Fotos,
  Explorer), descobrível pelo check do hover.
- **Ações em lote fazendo N chamadas no front**: funciona, mas dispara N recargas e não é
  atômico. Preferido mudar os comandos pra receber listas.
