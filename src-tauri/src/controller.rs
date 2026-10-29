use crate::{storage, vault::Vault};
use anyhow::{anyhow, ensure, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::OpenOptionsExt,
            net::{UnixListener, UnixStream},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
}
#[derive(Serialize, Deserialize)]
pub struct Request {
    pub op: String,
    #[serde(default)]
    pub data: Value,
}
pub struct Controller {
    vault: Vault,
    child: Option<Child>,
    runtime: PathBuf,
    last_activity: Instant,
    retry_at: Instant,
    failures: u32,
    lock_error: Option<String>,
}
fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v.get(k)
        .and_then(Value::as_str)
        .context("Campo obrigatório ausente")
}
pub fn rpc(runtime: &Path, request: Request) -> Result<Value> {
    let mut socket = UnixStream::connect(runtime.join("control.sock"))?;
    socket.set_read_timeout(Some(Duration::from_secs(90)))?;
    send(&mut socket, &Zeroizing::new(serde_json::to_vec(&request)?))?;
    let reply: Value = serde_json::from_slice(&receive(&mut socket)?)?;
    if let Some(e) = reply.get("error").and_then(Value::as_str) {
        return Err(anyhow!(e.to_string()));
    }
    Ok(reply["result"].clone())
}
fn send(s: &mut UnixStream, bytes: &[u8]) -> Result<()> {
    s.write_all(&(bytes.len() as u32).to_be_bytes())?;
    s.write_all(bytes)?;
    Ok(())
}
fn receive(s: &mut UnixStream) -> Result<Zeroizing<Vec<u8>>> {
    let mut n = [0; 4];
    s.read_exact(&mut n)?;
    let len = u32::from_be_bytes(n) as usize;
    ensure!(len <= 65536, "Requisição muito grande");
    let mut b = Zeroizing::new(vec![0; len]);
    s.read_exact(&mut b)?;
    Ok(b)
}
impl Controller {
    fn spawn(&mut self) -> Result<()> {
        let profile = if self.vault.master.is_some() && self.lock_error.is_none() {
            self.vault.mount.clone()
        } else {
            self.runtime.join("locked-ui")
        };
        storage::private_dir(&profile)?;
        let mut cmd = Command::new(std::env::current_exe()?);
        cmd.arg("--ui")
            .arg(&self.runtime)
            .arg(&profile)
            .process_group(0);
        cmd.env("XDG_CACHE_HOME", profile.join("cache"))
            .env("XDG_DATA_HOME", profile.join("data"));
        // No crash dumps, WebKit debugging, or user-provided sandbox bypasses.
        for name in [
            "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS",
            "WEBKIT_INSPECTOR_SERVER",
            "GTK_MODULES",
            "GTK3_MODULES",
        ] {
            cmd.env_remove(name);
        }
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
                Ok(())
            });
        }
        self.child = Some(cmd.spawn()?);
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        if let Some(mut c) = self.child.take() {
            let pg = c.id() as i32;
            unsafe {
                libc::kill(-pg, libc::SIGTERM);
            }
            let start = Instant::now();
            while c.try_wait()?.is_none() && start.elapsed() < Duration::from_secs(1) {
                std::thread::sleep(Duration::from_millis(50));
            }
            unsafe {
                libc::kill(-pg, libc::SIGKILL);
            }
            let _ = c.wait();
            // Unmount is authoritative: it must fail if another process still holds the mount.
        }
        Ok(())
    }
    fn lock(&mut self) -> Result<()> {
        self.stop()?;
        match self.vault.unmount() {
            Ok(()) => self.lock_error = None,
            Err(e) => self.lock_error = Some(e.to_string()),
        }
        self.spawn()?;
        Ok(())
    }
    fn accounts(&self) -> Result<Vec<Account>> {
        ensure!(
            self.vault.master.is_some() && self.lock_error.is_none(),
            "Cofre bloqueado"
        );
        let path = self.vault.mount.join("accounts.json");
        if path.exists() {
            storage::read(&path)
        } else {
            Ok(vec![])
        }
    }
    fn status(&self) -> Result<Value> {
        let m = &self.vault.manifest;
        Ok(
            json!({"created":m.is_some(), "unlocked":self.vault.master.is_some() && self.lock_error.is_none(), "pin":m.as_ref().is_some_and(|m|m.pin.is_some()), "keys":m.as_ref().map(|m|m.keys.iter().map(|k|json!({"id":k.id,"name":k.name})).collect::<Vec<_>>()).unwrap_or_default(), "accounts":self.accounts().unwrap_or_default(), "idleMinutes":m.as_ref().map(|m|m.idle_minutes).unwrap_or(0), "error":self.lock_error, "version":env!("CARGO_PKG_VERSION")}),
        )
    }
    fn handle(&mut self, r: &Request) -> Result<Value> {
        let d = &r.data;
        match r.op.as_str() {
            "status" => return self.status(),
            "activity" => {
                self.last_activity = Instant::now();
                return Ok(Value::Null);
            }
            "create" | "unlock" => {
                ensure!(crate::session_monitor::SESSION_SAFE.load(Ordering::Relaxed), "Sessão Linux bloqueada ou monitor de segurança indisponível. Desbloqueie a sessão e tente novamente.");
                ensure!(
                    self.lock_error.is_none(),
                    "Resolva a falha de desmontagem antes de continuar"
                );
                ensure!(
                    Instant::now() >= self.retry_at,
                    "Aguarde alguns segundos antes de tentar novamente"
                );
                let secret = Zeroizing::new(text(d, "secret")?.to_string());
                let result = if r.op == "create" {
                    self.vault.create(&secret)
                } else {
                    self.vault.unlock(
                        text(d, "method")?,
                        &secret,
                        d["keyId"].as_str().unwrap_or(""),
                    )
                };
                if result.is_err() {
                    self.failures = self.failures.saturating_add(1);
                    self.retry_at = Instant::now()
                        + Duration::from_secs((1u64 << self.failures.min(5)).min(30));
                }
                result?;
                self.failures = 0;
                self.last_activity = Instant::now();
            }
            "lock" | "quit" => {}
            _ => {
                ensure!(
                    self.vault.master.is_some() && self.lock_error.is_none(),
                    "Cofre bloqueado"
                );
                match r.op.as_str() {
                    "account.add" => {
                        let name = account_name(text(d, "name")?)?;
                        let mut list = self.accounts()?;
                        ensure!(list.len() < 20, "Limite de vinte contas");
                        list.push(Account {
                            id: uuid::Uuid::new_v4().to_string(),
                            name,
                        });
                        storage::write(&self.vault.mount.join("accounts.json"), &list)?;
                    }
                    "account.rename" => {
                        let mut list = self.accounts()?;
                        let name = account_name(text(d, "name")?)?;
                        list.iter_mut()
                            .find(|a| a.id == d["id"])
                            .context("Conta inexistente")?
                            .name = name;
                        storage::write(&self.vault.mount.join("accounts.json"), &list)?;
                    }
                    "account.remove" => {
                        let mut list = self.accounts()?;
                        let id = text(d, "id")?;
                        uuid::Uuid::parse_str(id)?;
                        ensure!(list.iter().any(|a| a.id == id), "Conta inexistente");
                        list.retain(|a| a.id != id);
                        storage::write(&self.vault.mount.join("accounts.json"), &list)?;
                        /* Profile deletion happens only after every worker exits. */
                    }
                    "account.reorder" => {
                        let list = self.accounts()?;
                        let ids: Vec<String> = serde_json::from_value(d["ids"].clone())?;
                        ensure!(ids.len() == list.len(), "Ordem inválida");
                        let mut ordered = Vec::new();
                        for id in ids {
                            ensure!(
                                !ordered.iter().any(|a: &Account| a.id == id),
                                "Conta repetida"
                            );
                            ordered.push(
                                list.iter()
                                    .find(|a| a.id == id)
                                    .context("Conta inexistente")?
                                    .clone(),
                            );
                        }
                        storage::write(&self.vault.mount.join("accounts.json"), &ordered)?;
                    }
                    "password" => self
                        .vault
                        .password(text(d, "password")?, text(d, "newPassword")?)?,
                    "pin" => self.vault.set_pin(text(d, "password")?, text(d, "pin")?)?,
                    "key.add" => self.vault.add_key(
                        text(d, "password")?,
                        text(d, "pin")?,
                        text(d, "name")?,
                    )?,
                    "key.remove" => {
                        self.vault.verify_password(text(d, "password")?)?;
                        self.vault
                            .manifest
                            .as_mut()
                            .unwrap()
                            .keys
                            .retain(|k| k.id != d["id"]);
                        self.vault.save()?;
                    }
                    "settings" => {
                        let minutes = d["idleMinutes"].as_u64().context("Tempo inválido")?;
                        ensure!(minutes <= 1440, "Tempo inválido");
                        self.vault.manifest.as_mut().unwrap().idle_minutes = minutes as u32;
                        self.vault.save()?;
                    }
                    _ => return Err(anyhow!("Operação não permitida")),
                }
            }
        }
        self.status()
    }
    fn prune(&self) -> Result<()> {
        let ids = self
            .accounts()?
            .into_iter()
            .map(|a| a.id)
            .collect::<Vec<_>>();
        let p = self.vault.mount.join("accounts");
        if !p.exists() {
            return Ok(());
        }
        for item in fs::read_dir(p)? {
            let item = item?;
            let id = item.file_name().to_string_lossy().into_owned();
            if uuid::Uuid::parse_str(&id).is_ok() && !ids.contains(&id) {
                fs::remove_dir_all(item.path())?;
            }
        }
        Ok(())
    }
}
fn account_name(s: &str) -> Result<String> {
    let s = s.trim();
    ensure!(
        !s.is_empty() && s.chars().count() <= 40 && !s.chars().any(char::is_control),
        "Use um nome entre 1 e 40 caracteres"
    );
    Ok(s.into())
}
pub fn run(base: PathBuf, runtime: PathBuf) -> Result<()> {
    storage::private_dir(&runtime)?;
    let guard = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(runtime.join("instance.lock"))?;
    if guard.try_lock_exclusive().is_err() {
        return Ok(());
    }
    let socket = runtime.join("control.sock");
    if socket.exists() {
        fs::remove_file(&socket)?;
    }
    let listener = UnixListener::bind(socket)?;
    listener.set_nonblocking(true)?;
    let vault = Vault::new(base, &runtime)?;
    let mut c = Controller {
        vault,
        child: None,
        runtime,
        last_activity: Instant::now(),
        retry_at: Instant::now(),
        failures: 0,
        lock_error: None,
    };
    // An interrupted previous run is never implicitly adopted.
    if c.vault.mounted() {
        if let Err(e) = c.vault.unmount() {
            c.lock_error = Some(e.to_string());
        }
    }
    let shutdown = Arc::new(AtomicBool::new(false));
    for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        signal_hook::flag::register(sig, shutdown.clone())?;
    }
    let lock = Arc::new(AtomicBool::new(false));
    crate::session_monitor::start(lock.clone(), shutdown.clone());
    c.spawn()?;
    loop {
        if shutdown.load(Ordering::Relaxed) {
            c.stop()?;
            c.vault.unmount()?;
            break;
        }
        let timeout = c
            .vault
            .manifest
            .as_ref()
            .map(|m| m.idle_minutes)
            .unwrap_or(0);
        if c.vault.master.is_some()
            && (lock.swap(false, Ordering::Relaxed)
                || (timeout > 0 && c.last_activity.elapsed().as_secs() >= u64::from(timeout) * 60))
        {
            c.lock()?;
        }
        if c.child
            .as_mut()
            .map(|p| p.try_wait())
            .transpose()?
            .flatten()
            .is_some()
        {
            c.lock()?;
        }
        if let Ok((mut socket, _)) = listener.accept() {
            socket.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut peer: libc::ucred = unsafe { std::mem::zeroed() };
            let mut len = std::mem::size_of_val(&peer) as libc::socklen_t;
            let ok = unsafe {
                libc::getsockopt(
                    socket.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_PEERCRED,
                    (&mut peer as *mut libc::ucred).cast(),
                    &mut len,
                )
            };
            // Only the current trusted UI process may call the controller, not a remote WebKit renderer.
            if ok != 0
                || peer.uid != unsafe { libc::getuid() }
                || c.child.as_ref().map(|p| p.id() as i32) != Some(peer.pid)
            {
                continue;
            }
            let result = receive(&mut socket)
                .and_then(|bytes| Ok(serde_json::from_slice::<Request>(&bytes)?));
            if let Ok(mut r) = result {
                if matches!(r.op.as_str(), "create" | "unlock")
                    && crate::session_monitor::SESSION_SAFE.load(Ordering::Relaxed)
                {
                    lock.store(false, Ordering::Relaxed);
                }
                let result = c.handle(&r);
                let success = result.is_ok();
                let response = match result {
                    Ok(v) => json!({"result":v}),
                    Err(e) => json!({"error":e.to_string()}),
                };
                let _ = send(&mut socket, &serde_json::to_vec(&response)?);
                if success {
                    match r.op.as_str() {
                        "create" | "unlock" | "account.remove" => {
                            c.stop()?;
                            c.prune()?;
                            c.spawn()?;
                        }
                        "lock" => c.lock()?,
                        "quit" => {
                            c.stop()?;
                            c.vault.unmount()?;
                            break;
                        }
                        _ => {}
                    }
                }
                // Drop secrets promptly; never include request objects in logs.
                scrub(&mut r.data);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
fn scrub(v: &mut Value) {
    use zeroize::Zeroize;
    match v {
        Value::String(s) => s.zeroize(),
        Value::Array(v) => v.iter_mut().for_each(scrub),
        Value::Object(m) => m.values_mut().for_each(scrub),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_mutations_while_locked() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let vault = Vault::new(dir.path().into(), runtime.path()).unwrap();
        let mut c = Controller {
            vault,
            child: None,
            runtime: runtime.path().into(),
            last_activity: Instant::now(),
            retry_at: Instant::now(),
            failures: 0,
            lock_error: None,
        };
        for op in [
            "account.add",
            "account.remove",
            "settings",
            "pin",
            "key.add",
            "password",
        ] {
            assert!(c
                .handle(&Request {
                    op: op.into(),
                    data: json!({})
                })
                .is_err());
        }
    }
    #[test]
    fn rejects_duplicate_account_order() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let mut vault = Vault::new(dir.path().into(), runtime.path()).unwrap();
        vault.master = Some(crate::crypto::random());
        let list = vec![
            Account {
                id: uuid::Uuid::new_v4().to_string(),
                name: "One".into(),
            },
            Account {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Two".into(),
            },
        ];
        storage::write(&vault.mount.join("accounts.json"), &list).unwrap();
        let mut c = Controller {
            vault,
            child: None,
            runtime: runtime.path().into(),
            last_activity: Instant::now(),
            retry_at: Instant::now(),
            failures: 0,
            lock_error: None,
        };
        assert!(c
            .handle(&Request {
                op: "account.reorder".into(),
                data: json!({"ids":[list[0].id,list[0].id]})
            })
            .is_err());
        assert_eq!(c.accounts().unwrap()[1].id, list[1].id);
    }
    #[test]
    fn rejects_oversize_rpc_before_allocating() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        a.write_all(&u32::MAX.to_be_bytes()).unwrap();
        assert!(receive(&mut b).is_err());
    }
}
