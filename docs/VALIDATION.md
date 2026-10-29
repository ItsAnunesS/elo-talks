# Validação

Este arquivo distingue verificações executadas de critérios ainda pendentes. Uma compilação bem-sucedida não certifica compatibilidade com o WhatsApp nem segurança do pacote.

## Automatizado

- Testes Rust: envelope autenticado, credencial incorreta, contexto incorreto, dados alterados, troca de senha sem troca de chave, gravação atômica, bloqueio de mutações e validação de RPC.
- Testes Python: versões sincronizadas e restrição de capabilities à interface local.
- Teste FUSE separado: criação, conteúdo cifrado, desmontagem, senha incorreta e restauração do conteúdo.
- CI: formatação, Clippy, testes, auditoria e build antes da publicação.

## Aceite manual obrigatório antes de considerar a versão pronta

| Cenário | Fedora 44 limpo | Ubuntu 24.04 limpo |
|---|---|---|
| Executar AppImage sem instalar pacotes | Pendente | Pendente |
| Criar cofre, fechar, reiniciar e desbloquear | Pendente | Pendente |
| QR code e duas contas reais isoladas | Pendente | Pendente |
| Mensagens após reinício e operação offline | Pendente | Pendente |
| Notificações com janela fechada e conta inativa por 30 min | Pendente | Pendente |
| Mídia, anexos e exportação explícita | Pendente | Pendente |
| Bloqueio de tela, suspensão e encerramento de sessão | Pendente | Pendente |
| PIN e chaveiro bloqueado/indisponível | Pendente | Pendente |
| YubiKey compatível: cadastro, desbloqueio, erro e remoção | Pendente | Pendente |
| Falha abrupta, disco cheio e montagem residual | Pendente | Pendente |
| Inspeção de gravações fora do cofre | Pendente | Pendente |

Não remover estes itens com base em testes de interface simulados. Exigem sessões de desktop, hardware e contas reais. Não desativar sandbox nem instalar dependências para marcar os critérios como concluídos.

## Roteiro FUSE

Com o binário de testes compilado e o helper empacotado, executar o teste `vault::tests::fuse_persistence_and_ciphertext` com `--ignored --exact` e `ELO_TEST_HELPER` apontando para o helper. O teste usa apenas diretórios temporários e dados sintéticos; não usa o cofre do usuário.

## Evidências desta implementação (2026-10-29)

- Compilação em container Ubuntu 24.04 com Rust 1.99.1 e Tauri 2.12.5.
- 8 testes Rust e 6 testes Python aprovados; Clippy aprovado para o app.
- Teste FUSE real aprovado no Fedora do desenvolvedor: criação, escrita, ausência do marcador em texto puro, senha errada, reabertura e restauração. Executado via serviço temporário do usuário porque o runner de comandos impõe NoNewPrivs.
- Testes de interface em Chromium: PIN, erro de autenticação, limpeza dos campos, nomes potencialmente maliciosos tratados como texto e adição de conta. Esses testes usam um backend simulado e não validam WebKit/WhatsApp.
- Auditoria Rust sem vulnerabilidades conhecidas reportadas; correção local verificável para RUSTSEC-2024-0429. Quatro avisos de manutenção permanecem documentados em SECURITY.md.
- AppImage gerada com WebKitGTK, mídia, libfido2 e helper gocryptfs incluídos.

O Fedora do desenvolvedor não é uma instalação limpa. Os testes acima não substituem os itens pendentes da tabela.

- Inicialização nativa da AppImage confirmada no Fedora, com dados temporários; não foram usadas contas reais.
- Testes criptográficos upstream do helper aprovados após atualizar dependências Go; govulncheck sem vulnerabilidades alcançáveis.
