# Multi-idioma (inglês, português, espanhol)

## Por quê

O nahshot hoje fala só português. Queremos que qualquer pessoa use o app no próprio
idioma, com inglês como padrão, e que adicionar um idioma novo depois seja só criar
um arquivo.

## Objetivos

- Três idiomas: inglês (`en`), português (`pt`) e espanhol (`es`).
- Padrão: o idioma do Windows, quando é um dos três; senão, inglês.
- Escolha manual em Configurações: Sistema, English, Português, Español. A troca vale
  na hora, em todas as janelas, na bandeja, no overlay e no toast.
- Todo texto visível traduzido: galeria, configurações, boas-vindas, menus, diálogos,
  erros, datas, bandeja, títulos das janelas, diálogos do updater, notificações,
  toolbar e dica do overlay, toast nativo.
- A pasta de álbuns no disco tem nome fixo em inglês (`Albums`), e a UI mostra o nome
  traduzido.

## Não-objetivos

- Idiomas da direita pra esquerda e CJK.
- Traduzir o nome da pasta no Explorer do Windows (`desktop.ini`).
- Traduzir logs: eles continuam em inglês.
- Traduzir mensagens de erro cruas do sistema operacional (`{e}`).

## Anti-objetivos

- Perder álbuns, etiquetas ou favoritos de quem já usa o app ao trocar `Álbuns` por
  `Albums`.
- Um idioma ficar incompleto sem ninguém perceber.

## Abordagens consideradas

1. **JSON por idioma, compartilhado entre UI e Rust** (escolhida). Fica uma fonte só e
   não precisa de dependência. Um teste garante que todos os idiomas têm as mesmas
   chaves e os mesmos placeholders, e que toda chave usada no código existe.
2. Dicionários separados em TS e em Rust. É simples, mas cada idioma fica em dois
   lugares.
3. Bibliotecas (i18next, fluent). Resolvem plural e formatação complexa, que o app
   não tem, e trazem dependências nos dois lados.

## Como funciona

### Arquivos de texto

`locales/en.json`, `locales/pt.json`, `locales/es.json` na raiz do repositório. Cada
um é um objeto plano `"chave.com.pontos": "texto"`:

- Placeholders `{nome}` são substituídos por `t(chave, { nome })`.
- Plural usa duas chaves irmãs, `chave.one` e `chave.other`. A UI escolhe com
  `Intl.PluralRules`; o Rust não precisa de plural.
- `en` é a referência: chave faltando em outro idioma cai no inglês, e chave faltando
  no inglês aparece como a própria chave.

### Rust: crate `nahshot-i18n`

- Embute os três JSONs com `include_str!` e os parseia uma vez (`OnceLock`).
- `enum Lang { En, Pt, Es }`, com o idioma atual num atômico global: `set_lang`,
  `lang`. É global porque overlay, toast, bandeja e comandos rodam em threads
  diferentes e todos leem o idioma na hora de desenhar.
- `t(key) -> String` e `tf(key, &[(name, value)]) -> String`.
- `system_lang()` lê o idioma de interface do Windows (`GetUserDefaultUILanguage`) e
  devolve `Pt`/`Es`/`En`.
- `nahshot-capture` (toolbar, dica do overlay, toast) e `src-tauri` usam essa crate.

### Configuração

`Settings.language`: `"system" | "en" | "pt" | "es"`, padrão `"system"`. Arquivos
antigos sem o campo ficam com `"system"`. O comando `set_language` salva, aplica o
idioma resolvido, reconstrói o menu da bandeja, troca os títulos e recarrega as
janelas abertas com o novo `?lang=`.

### UI web

- Cada janela recebe `?lang=en|pt|es` (já resolvido pelo Rust), igual ao `?theme=`.
- `ui/src/i18n.ts` importa os JSONs e expõe `lang`, `locale()` (`en-US`, `pt-BR`,
  `es-ES`, usado nas datas do `Intl`), `t`, `tn` (plural) e `localizeDocument()`.
- Texto estático do HTML usa atributos `data-i18n`, `data-i18n-placeholder`,
  `data-i18n-aria-label` e `data-i18n-title`. `localizeDocument()` preenche esses
  atributos e o `<html lang>`.
- Trocar o idioma recarrega a janela. É raro, e recarregar garante que nada fica no
  idioma antigo.
- `friendlyError` passa a devolver texto traduzido. Mensagens que o Rust gera para a
  UI (updater) já chegam traduzidas pelo Rust.
- `groupByDay` recebe os rótulos (hoje/ontem) e o locale, para continuar puro e
  testável.

### Pasta de álbuns: `Álbuns` → `Albums`

- `naming::albums_dir` passa a ser `root\Albums`, e `LEGACY_ALBUMS_DIR = "Álbuns"`.
- `Library::open` (e `relocate`, ao trocar de raiz) chama `migrate_albums_dir`:
  - se `Álbuns` não existe, não faz nada;
  - se `Albums` não existe, renomeia a pasta inteira e atualiza os caminhos no
    banco numa transação; se o banco falhar, desfaz a renomeação;
  - se as duas existem, move cada álbum (ou cada arquivo, quando o álbum existe dos
    dois lados) sem sobrescrever nada. O que colide fica em `Álbuns` e vai pro log.
    A pasta antiga é removida só se ficar vazia;
  - se a renomeação falhar (pasta em uso), a biblioteca continua usando `Álbuns`
    nesta sessão e tenta de novo na próxima abertura.
- Para isso a `Library` guarda a pasta de álbuns ativa (`albums_dir`), em vez de
  recalcular a partir da raiz.

## Testes

- Vitest: os três JSONs têm as mesmas chaves e os mesmos placeholders; toda chave
  usada em `t(...)`/`tn(...)` no TS e no Rust existe em `en.json`; `t`, `tn` e o
  fallback; `groupByDay` em pt e en.
- Rust: `nahshot-i18n` (parse, fallback, `tf`, idioma global); migração da pasta
  (renomeação simples, já migrada, fusão com colisão, banco atualizado e tags
  preservadas); `Settings` antigo sem `language`.
- Manual: rodar o app nos três idiomas e trocar o idioma com as janelas abertas.

## Decisões tomadas na implementação

- Plural: `.one` quando a contagem é 1, `.other` nos outros casos, nos três idiomas.
  É o comportamento que o app já tinha em português ("0 prints").
- "Selecionar todos de hoje" virou "Selecionar todos: Hoje". Assim nenhum idioma
  precisa mudar maiúscula/minúscula da data.
- Em inglês e espanhol o app fala "screenshot" e "captura"; "print" fica só no
  português.
- A janela de Configurações ficou 80 px mais alta por causa da linha de idioma.
- O teste de chaves (`ui/src/i18n.test.ts`) lê o TS, os HTMLs e o Rust (`t(...)`,
  `tf(...)`). Ele falha quando uma chave usada não existe e quando uma chave dos
  JSONs não é usada.

## Decisões deixadas pra quem implementa

- Texto exato das traduções. O português é o texto atual; o inglês e o espanhol
  seguem o mesmo tom curto.
- Nomes exatos das chaves.
