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
- 8 testes Rust e 8 testes Python aprovados; Clippy aprovado para o app.
- Teste FUSE real aprovado no Fedora do desenvolvedor: criação, escrita, ausência do marcador em texto puro, senha errada, reabertura e restauração. Executado via serviço temporário do usuário porque o runner de comandos impõe NoNewPrivs.
- Testes de interface em Chromium: PIN, erro de autenticação, limpeza dos campos, nomes potencialmente maliciosos tratados como texto e adição de conta. Esses testes usam um backend simulado e não validam WebKit/WhatsApp.
- Auditoria Rust sem vulnerabilidades conhecidas reportadas; correção local verificável para RUSTSEC-2024-0429. Quatro avisos de manutenção permanecem documentados em SECURITY.md.
- AppImage gerada com WebKitGTK, mídia, libfido2 e helper gocryptfs incluídos.

O Fedora do desenvolvedor não é uma instalação limpa. Os testes acima não substituem os itens pendentes da tabela.

- Inicialização nativa da AppImage confirmada no Fedora, com dados temporários; não foram usadas contas reais.
- Testes criptográficos upstream do helper aprovados após atualizar dependências Go; govulncheck sem vulnerabilidades alcançáveis.

## Correções durante o teste do sandbox

A primeira inicialização usava o padrão do WebKitGTK 4.1, que não habilita sandbox. Ao torná-lo obrigatório, o teste revelou auxiliares ausentes e caminhos relativos incompatíveis com o namespace. `bwrap`, `xdg-dbus-proxy` e um adaptador de caminhos são agora empacotados. O adaptador preserva filtros seccomp e namespaces e restaura as bibliotecas de `/usr` e adiciona o diretório do aplicativo, ambos em modo somente leitura.

Uma falha da interface não reinicia mais janelas: encerra os processos, fecha o cofre e termina o app. Os testes posteriores são executados em desktop virtual separado do GNOME do usuário.

- Interface nativa renderizada e capturada em Xvfb dentro de container, com sandbox WebKit obrigatório e renderização Mesa por software. Captura local em `dist/native-test/native.png`. O teste não abriu janelas no GNOME.

## Layout integrado das contas

O contêiner GtkBox usado pelo Tauri/Wry no Linux não aplica os limites de posição das webviews filhas. A interface local e as contas agora compartilham um GtkOverlay: a interface ocupa a janela, e cada conta recebe margens nativas de 304 px à esquerda e 72 px no topo. O GTK gerencia redimensionamento e escala. O fullscreen solicitado por conteúdo remoto é desabilitado e seu sinal é interceptado.

O exemplo `layout_smoke` testa a composição real Tauri/WebKit com duas páginas sintéticas, sem contas nem dados pessoais. Validado em Xvfb isolado com sandbox WebKit obrigatório, em escalas de 100% e 200%: alternância, retorno à visão geral, redução para 900×640, expansão para 1400×900, limites das contas e recusa de fullscreen. O workflow executa essa regressão antes do empacotamento. O teste não substitui a validação de conversas, anexos e notificações com contas WhatsApp reais.


### Restrição AppArmor no runner Ubuntu

O teste nativo no GitHub Actions falhou com `bwrap: setting up uid map: Permission denied`. A restrição de user namespaces do Ubuntu impede esse executável de teste, sem perfil AppArmor, de iniciar o sandbox. O runner recebe um perfil temporário apenas para o caminho exato de `layout_smoke`, com a permissão `userns`; o script remove o perfil ao terminar, inclusive em falha. Não há alteração de sysctl nem desativação do sandbox WebKit. O perfil não integra a AppImage.

**Esse teste valida o layout sob uma permissão específica de CI, não a compatibilidade de instalação padrão do Ubuntu 24.04.** A execução sem configuração administrativa nesse sistema permanece pendente e exige validação/revisão da arquitetura antes de anunciar suporte certificado. Referência: [restrições de user namespaces do Ubuntu](https://documentation.ubuntu.com/release-notes/24.04/#unprivileged-user-namespace-restrictions).
