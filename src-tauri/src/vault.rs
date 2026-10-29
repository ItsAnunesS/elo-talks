use crate::{
    auth,
    crypto::{self, Envelope},
    storage,
};
use anyhow::{anyhow, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

#[derive(Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub password: Envelope,
    pub pin: Option<Envelope>,
    pub keys: Vec<auth::HardwareKey>,
    pub idle_minutes: u32,
}
pub struct Vault {
    pub base: PathBuf,
    pub mount: PathBuf,
    pub manifest: Option<Manifest>,
    pub master: Option<Zeroizing<Vec<u8>>>,
    pub fuse: Option<Child>,
    pub helper: PathBuf,
}
impl Vault {
    pub fn new(base: PathBuf, runtime: &Path) -> Result<Self> {
        storage::private_dir(&base)?;
        let mount = runtime.join("vault");
        storage::private_dir(&mount)?;
        let manifest = if base.join("vault.json").exists() {
            Some(storage::read::<Manifest>(&base.join("vault.json"))?)
        } else {
            None
        };
        if let Some(m) = &manifest {
            ensure!(m.schema == 1, "Version du coffre incompatible");
            uuid::Uuid::parse_str(&m.id)?;
        }
        let exe = std::env::current_exe()?;
        let helper = if cfg!(debug_assertions) {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("helpers/gocryptfs")
        } else {
            exe.parent().unwrap().join("gocryptfs")
        };
        Ok(Self {
            base,
            mount,
            manifest,
            master: None,
            fuse: None,
            helper,
        })
    }
    pub fn mounted(&self) -> bool {
        fs::read_to_string("/proc/self/mountinfo")
            .unwrap_or_default()
            .lines()
            .any(|line| {
                line.split_whitespace()
                    .nth(4)
                    .map(|s| {
                        s.replace("\\040", " ").replace("\\134", "\\")
                            == self.mount.to_string_lossy()
                    })
                    .unwrap_or(false)
            })
    }
    pub fn save(&self) -> Result<()> {
        storage::write(
            &self.base.join("vault.json"),
            self.manifest.as_ref().context("Cofre não criado")?,
        )
    }
    fn command(&self, master: &[u8], init: bool) -> Result<Child> {
        ensure!(
            self.helper.is_file(),
            "Componente do cofre ausente no pacote"
        );
        ensure!(
            Path::new("/dev/fuse").exists(),
            "Este sistema não oferece FUSE para abrir o cofre"
        );
        let mut c = Command::new(&self.helper);
        c.args(["-q", "-passfile", "/dev/stdin"]);
        if init {
            c.arg("-init");
        } else {
            c.args(["-fg", "-nosyslog"]);
        }
        c.arg(self.base.join("cipher"));
        if !init {
            c.arg(&self.mount);
        }
        c.stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        unsafe {
            c.pre_exec(|| {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = c.spawn().context("Não foi possível iniciar o cofre")?;
        let pass = Zeroizing::new(format!("{}\n", hex::encode(master)));
        if let Err(e) = child.stdin.take().unwrap().write_all(pass.as_bytes()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.into());
        }
        Ok(child)
    }
    pub fn create(&mut self, password: &str) -> Result<()> {
        ensure!(self.manifest.is_none(), "O cofre já existe");
        validate_password(password)?;
        ensure!(
            !self.base.join("cipher").exists(),
            "Inicialização anterior incompleta. Preserve a pasta cipher antes de tentar novamente."
        );
        storage::private_dir(&self.base.join("cipher"))?;
        let master = crypto::random();
        let id = uuid::Uuid::new_v4().to_string();
        let m = Manifest {
            schema: 1,
            password: crypto::wrap(&master, password.as_bytes(), &format!("{id}:password"))?,
            id,
            pin: None,
            keys: vec![],
            idle_minutes: 0,
        };
        // Save the wrapped key first: failure during initialization never loses the only key.
        self.manifest = Some(m);
        self.save()?;
        ensure!(
            self.command(&master, true)?.wait()?.success(),
            "Falha ao inicializar cofre. Nenhum perfil foi criado."
        );
        self.mount_key(master)
    }
    pub fn unlock(&mut self, method: &str, secret: &str, key_id: &str) -> Result<()> {
        ensure!(self.master.is_none(), "Cofre já aberto");
        let m = self.manifest.as_ref().context("Crie o cofre primeiro")?;
        let (envelope, material, context) = match method {
            "password" => (
                &m.password,
                Zeroizing::new(secret.as_bytes().to_vec()),
                format!("{}:password", m.id),
            ),
            "pin" => (
                m.pin.as_ref().context("PIN não configurado")?,
                auth::pin_secret(&m.id, secret, false)?,
                format!("{}:pin", m.id),
            ),
            "key" => {
                let k = m
                    .keys
                    .iter()
                    .find(|k| k.id == key_id)
                    .context("Chave não cadastrada")?;
                (
                    &k.envelope,
                    auth::hardware(false, &k.credential, &hex::decode(&k.salt)?, secret)?.1,
                    format!("{}:key:{}", m.id, k.id),
                )
            }
            _ => return Err(anyhow!("Método inválido")),
        };
        self.mount_key(crypto::unwrap(envelope, &material, &context)?)
    }
    fn mount_key(&mut self, master: Zeroizing<Vec<u8>>) -> Result<()> {
        ensure!(
            !self.mounted(),
            "Montagem residual detectada; encerre a instância anterior antes de abrir o cofre"
        );
        let mut child = self.command(&master, false)?;
        let start = Instant::now();
        while !self.mounted() {
            if child.try_wait()?.is_some() {
                return Err(anyhow!("Não foi possível montar o cofre neste sistema"));
            }
            if start.elapsed() > Duration::from_secs(15) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!("Tempo de montagem excedido"));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        self.fuse = Some(child);
        self.master = Some(master);
        Ok(())
    }
    pub fn unmount(&mut self) -> Result<()> {
        if self.mounted() {
            let status = Command::new("/usr/bin/fusermount3")
                .arg("-u")
                .arg(&self.mount)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .context("Não foi possível desmontar o cofre")?;
            ensure!(
                status.success() && !self.mounted(),
                "Cofre ainda montado. Feche os processos que o utilizam e tente novamente."
            );
        }
        if let Some(mut child) = self.fuse.take() {
            let _ = child.wait();
        }
        self.master = None;
        Ok(())
    }
    pub fn password(&mut self, old: &str, new: &str) -> Result<()> {
        validate_password(new)?;
        self.verify_password(old)?;
        let m = self.manifest.as_mut().unwrap();
        m.password = crypto::wrap(
            self.master.as_ref().context("Cofre bloqueado")?,
            new.as_bytes(),
            &format!("{}:password", m.id),
        )?;
        self.save()
    }
    pub fn verify_password(&self, password: &str) -> Result<()> {
        let m = self.manifest.as_ref().context("Cofre não criado")?;
        crypto::unwrap(
            &m.password,
            password.as_bytes(),
            &format!("{}:password", m.id),
        )?;
        Ok(())
    }
    pub fn set_pin(&mut self, password: &str, pin: &str) -> Result<()> {
        self.verify_password(password)?;
        let master = self.master.as_ref().context("Cofre bloqueado")?;
        let m = self.manifest.as_mut().unwrap();
        m.pin = if pin.is_empty() {
            None
        } else {
            Some(crypto::wrap(
                master,
                &auth::pin_secret(&m.id, pin, true)?,
                &format!("{}:pin", m.id),
            )?)
        };
        self.save()
    }
    pub fn add_key(&mut self, password: &str, pin: &str, name: &str) -> Result<()> {
        self.verify_password(password)?;
        ensure!(!name.trim().is_empty() && name.len() <= 80, "Nome inválido");
        let master = self.master.as_ref().context("Cofre bloqueado")?;
        let m = self.manifest.as_mut().unwrap();
        ensure!(m.keys.len() < 8, "Limite de oito chaves");
        let id = uuid::Uuid::new_v4().to_string();
        let salt = crypto::random();
        let (credential, material) = auth::hardware(true, "", &salt, pin)?;
        let envelope = crypto::wrap(master, &material, &format!("{}:key:{id}", m.id))?;
        m.keys.push(auth::HardwareKey {
            id,
            name: name.into(),
            credential,
            salt: hex::encode(&*salt),
            envelope,
        });
        self.save()
    }
}
fn validate_password(p: &str) -> Result<()> {
    ensure!(
        p.chars().count() >= 12 && p.len() <= 1024,
        "Use uma senha de pelo menos 12 caracteres"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn password_rotation_preserves_master_and_rejects_old_password() {
        let d = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let mut v = Vault::new(d.path().to_owned(), runtime.path()).unwrap();
        let master = crypto::random();
        v.manifest = Some(Manifest {
            schema: 1,
            id: "test".into(),
            password: crypto::wrap(&master, b"original password", "test:password").unwrap(),
            pin: None,
            keys: vec![],
            idle_minutes: 0,
        });
        v.master = Some(master.clone());
        assert!(v
            .password("wrong password", "replacement password")
            .is_err());
        v.password("original password", "replacement password")
            .unwrap();
        assert!(v.verify_password("original password").is_err());
        v.verify_password("replacement password").unwrap();
        assert_eq!(*v.master.unwrap(), *master);
    }
    #[test]
    fn malformed_envelope_cannot_panic_or_unlock() {
        let e = Envelope {
            salt: "00".into(),
            nonce: "00".into(),
            ciphertext: "00".into(),
        };
        assert!(crypto::unwrap(&e, b"password", "context").is_err());
    }
    #[test]
    #[ignore = "requires host FUSE and ELO_TEST_HELPER pointing to the pinned helper"]
    fn fuse_persistence_and_ciphertext() {
        let d = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let mut v = Vault::new(d.path().to_owned(), runtime.path()).unwrap();
        v.helper = PathBuf::from(std::env::var("ELO_TEST_HELPER").unwrap());
        v.create("correct horse battery staple").unwrap();
        fs::write(
            v.mount.join("sensitive.txt"),
            b"elo-confidential-marker-934857",
        )
        .unwrap();
        v.unmount().unwrap();
        for entry in fs::read_dir(v.base.join("cipher")).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() {
                assert!(!fs::read(p)
                    .unwrap()
                    .windows(28)
                    .any(|w| w == b"elo-confidential-marker-934857"));
            }
        }
        assert!(v.unlock("password", "incorrect password", "").is_err());
        assert!(!v.mounted());
        v.unlock("password", "correct horse battery staple", "")
            .unwrap();
        assert_eq!(
            fs::read(v.mount.join("sensitive.txt")).unwrap(),
            b"elo-confidential-marker-934857"
        );
        v.unmount().unwrap();
        assert!(!v.mounted());
    }
}
