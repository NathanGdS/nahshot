# AGENTS.md

Instruções para agentes de IA trabalhando neste repositório.

## Projeto

nahshot: app de captura de tela para Windows (estilo Lightshot) com galeria organizada. Stack: Rust + Tauri v2.

## Specs

Todas as specs ficam em `docs/specs/`, uma pasta por spec:

```
docs/specs/<titulo-da-spec>/
  design.md      # design / o quê e porquê
  plan.md        # plano de implementação (quando houver)
  ...            # outros documentos da spec
```

- `<titulo-da-spec>` em kebab-case, curto e descritivo (ex.: `nahshot-v1`, `editar-print-antigo`).
- Não usar `docs/superpowers/specs/` nem outro local: sempre `docs/specs/<titulo>/`.
- Todo documento novo de uma spec (design, plano, notas, decisões) vai dentro da pasta dela.

## Commits

- Nunca adicionar `Co-Authored-By` nas mensagens de commit.
