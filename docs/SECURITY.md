# Modelo de segurança

O Elo Talks é um cliente não oficial que carrega o WhatsApp Web. A criptografia de ponta a ponta das conversas pertence ao WhatsApp; o aplicativo acrescenta isolamento de contas e proteção local em repouso.

## Cofre

O perfil completo de cada conta (inclusive cookies em texto e IndexedDB produzidos pelo WebKit) deve permanecer dentro da montagem gocryptfs. Os arquivos persistidos fora dela são metadados de formato, envelopes de chaves e configurações não relacionadas a conversas. Nomes de chaves físicas ficam nos metadados do cofre; não use informações confidenciais nesses nomes.

Uma chave aleatória de 256 bits, transmitida por stdin ao gocryptfs, abre o filesystem. Os métodos de desbloqueio protegem cópias dessa chave usando AES-256-GCM, com nonce aleatório e contexto autenticado por cofre e método. Argon2id usa 64 MiB, três iterações e paralelismo um. A senha principal deve ter no mínimo 12 caracteres.

O PIN de seis dígitos é combinado com um segredo de 256 bits mantido no Secret Service persistente do Linux, acessado por sessão DH. Não existe fallback de chaveiro em arquivo ou segredo fixo. O PIN não protege contra um invasor que controle a sessão Linux desbloqueada e obtenha esse segredo. Nesse cenário, seis dígitos continuam sendo um espaço pequeno de busca. O atraso da interface não é defesa contra ataque offline.

Chaves físicas precisam de FIDO2 hmac-secret, presença e verificação do usuário. Apenas uma chave deve estar conectada durante a operação. O segredo do autenticador abre um envelope; uma mera assinatura de autenticação não é usada como chave de criptografia. O helper de hardware é cancelado se a sessão Linux deixar de estar segura. A senha principal permanece alternativa de recuperação. Não há servidor de recuperação.

## Limites

- O conteúdo existe descriptografado na memória e na visão montada enquanto o cofre está aberto.
- Root, malware com privilégios do usuário, swap ou hibernação sem criptografia e comprometimento do WhatsApp/WebKit ficam fora da garantia de proteção em repouso.
- Arquivos exportados após seleção de destino ficam fora do cofre. O usuário é responsável por essas cópias.
- Tamanhos, quantidade de arquivos, horários e versões anteriores podem revelar metadados. Criptografia autenticada não impede restauração de uma cópia antiga completa.
- Exclusão lógica não garante apagamento físico de SSDs, snapshots ou backups. Revogue aparelhos conectados pelo celular ao remover uma conta.
- Dumps produzidos pelo processo são desativados. Isso não controla ferramentas privilegiadas de captura do sistema.

## Fronteiras de processos

O controlador não carrega conteúdo web. Só aceita RPC pelo socket privado da sessão, conferindo UID e PID do processo de interface atual. Cada webview remota usa contexto e pasta próprios e não recebe capabilities Tauri. Os comandos conferem também label e URL da webview local. O processo de interface é encerrado antes da desmontagem; uma desmontagem malsucedida é exibida como falha, nunca como cofre protegido.

Mudanças na lista de contas são atômicas e sincronizadas. Remoção fecha os processos antes de apagar o perfil. Falha de criação preserva o envelope para não perder a chave de um filesystem parcialmente inicializado.

## Distribuição

As Actions são fixadas por SHA; Cargo.lock fixa dependências transitivas. O build verifica vulnerabilidades conhecidas com cargo-audit. Essa análise não prova ausência de falhas e não cobre automaticamente todos os componentes nativos. O inventário publicado inclui dependências Rust, Go e pacotes do build.

A assinatura Sigstore de SHA256SUMS protege também o arquivo de procedência incluído na lista. Verifique identidade e emissor, não apenas a validade matemática da assinatura:

```sh
cosign verify-blob --bundle SHA256SUMS.sigstore.json \
  --certificate-identity 'https://github.com/ItsAnunesS/elo-talks/.github/workflows/release.yml@refs/heads/release' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' SHA256SUMS
sha256sum -c SHA256SUMS
```

Não há certificação de segurança nem promessa de bibliotecas sem falhas. Consulte VALIDATION.md antes de usar dados reais.

## Avisos encontrados durante a implementação

`RUSTSEC-2024-0429` afeta GLib 0.18.5, necessário à linha GTK3 do Tauri. O projeto inclui a correção exata do upstream em `vendor/glib`, com teste de regressão e checksum do arquivo corrigido. A exceção de auditoria vale somente para esse aviso corrigido localmente. Novos avisos de comportamento indefinido bloqueiam a publicação.

A auditoria também informa dependências sem manutenção (`proc-macro-error`, `zbus_names`, `zvariant`, `zvariant_derive`). Esses avisos permanecem visíveis e representam dívida de manutenção; não são apresentados como bibliotecas comprovadamente livres de falhas.

O helper é compilado a partir de gocryptfs 2.6.1 (revisão fixa) com atualizações explícitas de `x/crypto`, `x/sys` e `x/term`. Seus testes de criptografia, conteúdo e nomes são executados antes de empacotar. `govulncheck` analisa o programa com as mesmas build tags; a análise registra um aviso de OpenPGP no módulo x/crypto, mas o programa não importa nem alcança esse pacote. Não há exceção de auditoria Go. As versões e checksums das ferramentas de empacotamento também são fixados; se um download mutável mudar, o build deve falhar até revisão.

## Sandbox na AppImage

WebKitGTK 4.1 precisa de ativação explícita. O controlador define `WEBKIT_FORCE_SANDBOX=1`, remove o bypass de sandbox e a webview remota só navega após confirmar que o contexto está protegido. O pacote inclui bubblewrap e xdg-dbus-proxy. `scripts/bwrap.sh` adapta os caminhos relativos produzidos por linuxdeploy, preserva namespaces e seccomp e monta somente para leitura o bundle e `/usr` (bibliotecas do sistema que o sandbox WebKit normalmente expõe). Não monta o diretório pessoal nem o cofre por esse adaptador.

Uma falha inesperada da interface encerra a aplicação e fecha o cofre; não há reinicialização automática. O helper do cofre recebe sinal de término se o controlador morrer. Falhas na desmontagem continuam sendo reportadas como erro.
