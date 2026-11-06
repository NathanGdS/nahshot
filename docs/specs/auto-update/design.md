# Auto-update

## O quê

O nahshot procura uma versão nova sozinho e se atualiza sem o usuário baixar nada.

- **Ao abrir** (só em build de release): procura em segundo plano. Se houver versão
  nova, pergunta "Atualizar agora / Depois". Se aceitar, baixa, fecha, instala e
  abre de novo. Falha de rede só vai para o log.
- **Configurações → Atualizações**: botão "Procurar atualizações". Mostra se está na
  versão mais recente ou pede confirmação e atualiza, com o progresso no botão.

## Como

Usa o `tauri-plugin-updater`, sem `gh`, sem token e sem nada instalado na máquina do usuário.

1. O app lê `https://github.com/NathanGdS/nahshot/releases/latest/download/latest.json`
   (o repo é público). `releases/latest` ignora prereleases, então só versões
   estáveis chegam aos usuários.
2. O `latest.json` traz a versão, a URL do instalador NSIS e a assinatura minisign dele.
3. O plugin baixa o instalador e confere a assinatura com a chave pública em
   `tauri.conf.json` (`plugins.updater.pubkey`). Se a assinatura não bater, ele não instala.
4. Roda o instalador em modo `passive` (`/P /R`: só uma barra de progresso, reabre o
   app no fim) e encerra o processo atual.

Código: `src-tauri/src/updater.rs` (checagem no startup e os comandos `check_update` /
`install_update`), seção em `ui/src/settings.ts`.

## Release

`.github/workflows/release.yml`, a cada tag `v*`:

- liga `bundle.createUpdaterArtifacts` só no CI. Build local não assina e não precisa da chave;
- assina o instalador com o secret `TAURI_SIGNING_PRIVATE_KEY` (chave sem senha);
- publica `…-setup.exe.sig` e `latest.json` junto do instalador.

## Chave de assinatura

- Privada: fora do repo (`~/.tauri/nahshot.key` na máquina de quem gerou), copiada para o
  secret `TAURI_SIGNING_PRIVATE_KEY` do repo. **Se perder, as versões instaladas não
  aceitam mais updates.** Guarde backup.
- Pública: `tauri.conf.json`. Trocar a chave exige que o usuário reinstale na mão uma vez.

## Limites

- A v0.1.0 não tem updater: quem tem ela instala a próxima versão na mão uma vez.
- Build de debug só checa, nunca instala.
