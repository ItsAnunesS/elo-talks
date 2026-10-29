# Elo Talks

App desktop Linux em Tauri 2 para múltiplas contas do WhatsApp Web, com perfis persistentes dentro de um cofre criptografado.

**Estado: implementação inicial em validação.** Não use a existência de uma AppImage como certificação de segurança ou de compatibilidade. Os critérios pendentes estão em [docs/VALIDATION.md](docs/VALIDATION.md).

## Uso

Baixe a AppImage em Releases, verifique a assinatura e os checksums conforme [docs/SECURITY.md](docs/SECURITY.md), torne o arquivo executável e abra-o. O alvo inicial é Fedora 44 e Ubuntu 24.04 x86_64, sem instalação adicional de pacotes.

Na primeira abertura, crie uma senha de pelo menos 12 caracteres. Adicione as contas e escaneie cada QR code pelo celular. O app mantém contas separadas e oferece configurações de PIN local, chave física compatível e timeout de bloqueio. A senha principal continua sendo a alternativa de recuperação. Sem ela e sem outro método cadastrado, não há recuperação do cofre.

Fechar a janela mantém o app na bandeja. Bloquear o desktop ou suspender fecha as sessões e o cofre. Se o ambiente não oferecer uma bandeja utilizável, a janela deve continuar acessível. Exporte anexos somente para locais onde queira manter uma cópia fora do cofre.

A inicialização automática registra o caminho original da AppImage. Mantenha o arquivo nesse local; ao substituí-lo por outra versão, abra a nova AppImage uma vez para atualizar o registro. O arquivo de autostart fica em `~/.config/autostart/dev.anuness.elotalks.desktop` e pode ser desativado pelas configurações do desktop. O registro do identificador anterior é removido ao abrir a nova versão.

## Atualizações pelo gerenciador de AppImages

As novas AppImages incluem a fonte de atualização padrão `gh-releases-zsync`, apontando para a release estável mais recente de `ItsAnunesS/elo-talks`. Gear Lever e outros gerenciadores compatíveis podem descobrir as versões sem preencher URLs manualmente. Cada release publica a AppImage e seu `.AppImage.zsync` correspondente, ambos cobertos por `SHA256SUMS` e pela assinatura Sigstore dessa lista.

Se a versão instalada ainda não contém esses metadados, substitua-a uma vez pela nova AppImage e recarregue os metadados no Gear Lever. Mantenha a fonte padrão, sem uma substituição manual configurada. Encerre o Elo Talks pela opção **Sair** antes de substituir o arquivo; fechar apenas a janela pode mantê-lo na bandeja. As contas continuam no cofre separado do executável.

A descoberta de atualizações usa o mecanismo do gerenciador, sem um atualizador dentro do Elo Talks. O reconhecimento de `.zsync` não implica verificação automática de Sigstore pelo gerenciador; a verificação da assinatura continua descrita em [SECURITY.md](docs/SECURITY.md).

## Desenvolvimento

Ferramentas necessárias **somente para desenvolver**: Rust conforme `rust-toolchain.toml`, Go 1.26.10, compilador C, pkg-config, WebKitGTK 4.1, GTK3, libfido2, libdbus, AppIndicator, binutils e zsync. A interface é HTML/CSS/JavaScript local, sem npm nem scripts remotos.

```sh
bash scripts/prepare-helper.sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
python3 -m unittest discover -s tests -v
cargo install tauri-cli --version 2.12.4 --locked
bash scripts/prepare-bundler.sh
bash scripts/build-appimage.sh
```

Para execução de desenvolvimento, compile o app com `cargo build`, copie o helper para o diretório configurado e execute `target/debug/elo-talks` numa sessão gráfica Linux. O controlador precisa de logind e FUSE; não desative sandbox para contornar incompatibilidades.

## Estrutura

- `ui/`: interface local, lista de contas, desbloqueio e configurações.
- `src-tauri/src/`: controlador, cofre, envelopes de chaves, autenticação, monitor da sessão e WebKit.
- `vendor/glib/`: backport documentado de uma correção de segurança upstream.
- `.github/workflows/release.yml`: versão `0.1.<run_number>`, testes, auditoria, AppImage e release por push em `release`.

Cada conta usa um contexto WebKit e diretório próprios. O conteúdo remoto não recebe capabilities Tauri. As requisições locais passam por validação de origem e por um socket privado com conferência de UID/PID. Não há backend, telemetria, chamadas de voz/vídeo ou atualizador interno nesta versão.

WhatsApp é uma marca de seu proprietário. Este projeto não é afiliado ao WhatsApp ou à Meta.
