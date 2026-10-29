use crate::crypto::{self, Envelope};
use anyhow::{anyhow, ensure, Result};
use secret_service::{blocking::SecretService, EncryptionType};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, ffi::CString};
use zeroize::Zeroizing;

#[derive(Clone, Serialize, Deserialize)]
pub struct HardwareKey {
    pub id: String,
    pub name: String,
    pub credential: String,
    pub salt: String,
    pub envelope: Envelope,
}
extern "C" {
    fn elo_fido(
        enroll: i32,
        challenge: *const u8,
        salt: *const u8,
        user: *const u8,
        credential: *mut u8,
        len: *mut usize,
        pin: *const libc::c_char,
        secret: *mut u8,
    ) -> i32;
}
pub fn hardware_native(
    enroll: bool,
    credential: &str,
    salt: &[u8],
    pin: &str,
) -> Result<(String, Zeroizing<Vec<u8>>)> {
    ensure!(salt.len() == 32, "Chave inválida");
    let mut cred = if enroll {
        vec![0; 2048]
    } else {
        hex::decode(credential)?
    };
    let mut len = cred.len();
    let challenge = crypto::random();
    let user = crypto::random();
    let pin = Zeroizing::new(CString::new(pin)?.into_bytes_with_nul());
    let mut out = Zeroizing::new(vec![0; 32]);
    let code = unsafe {
        elo_fido(
            enroll as i32,
            challenge.as_ptr(),
            salt.as_ptr(),
            user.as_ptr(),
            cred.as_mut_ptr(),
            &mut len,
            pin.as_ptr().cast(),
            out.as_mut_ptr(),
        )
    };
    ensure!(code == 0, "Não foi possível usar a chave (código {code}). Conecte somente uma chave FIDO2 compatível, informe seu PIN e toque nela.");
    cred.truncate(len);
    Ok((hex::encode(cred), out))
}
pub fn keyring(vault: &str, create: bool) -> Result<Zeroizing<Vec<u8>>> {
    let service = SecretService::connect(EncryptionType::Dh)
        .map_err(|_| anyhow!("Chaveiro seguro indisponível. Use sua senha."))?;
    let collection = service.get_default_collection()?;
    ensure!(
        !collection.is_locked()?,
        "Desbloqueie o chaveiro do Linux ou use sua senha."
    );
    // Never use a transient session collection.
    ensure!(
        !collection.collection_path.as_str().ends_with("/session"),
        "Chaveiro persistente indisponível"
    );
    // Keep the keyring namespace stable across app ID changes so existing PINs work.
    let attrs = HashMap::from([("application", "com.elotalks.desktop"), ("vault", vault)]);
    let items = collection.search_items(attrs.clone())?;
    if let Some(item) = items.first() {
        let s = Zeroizing::new(item.get_secret()?);
        ensure!(s.len() == 32, "Segredo local inválido");
        return Ok(s);
    }
    ensure!(
        create,
        "PIN não disponível neste computador. Use sua senha."
    );
    let s = crypto::random();
    collection.create_item(
        "Elo Talks — PIN deste computador",
        attrs,
        &s,
        false,
        "application/octet-stream",
    )?;
    Ok(s)
}
pub fn pin_secret(vault: &str, pin: &str, create: bool) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(
        pin.len() == 6 && pin.bytes().all(|c| c.is_ascii_digit()),
        "O PIN precisa ter seis dígitos"
    );
    let mut s = keyring(vault, create)?;
    s.extend_from_slice(pin.as_bytes());
    Ok(s)
}

#[derive(Serialize, Deserialize)]
struct HardwareRequest {
    enroll: bool,
    credential: String,
    salt: Vec<u8>,
    pin: String,
}
pub fn hardware(
    enroll: bool,
    credential: &str,
    salt: &[u8],
    pin: &str,
) -> Result<(String, Zeroizing<Vec<u8>>)> {
    use std::{
        io::{Read, Write},
        process::{Command, Stdio},
        sync::atomic::Ordering,
        time::{Duration, Instant},
    };
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--fido-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut request = HardwareRequest {
        enroll,
        credential: credential.into(),
        salt: salt.into(),
        pin: pin.into(),
    };
    let bytes = Zeroizing::new(serde_json::to_vec(&request)?);
    use zeroize::Zeroize;
    request.pin.zeroize();
    child.stdin.take().unwrap().write_all(&bytes)?;
    let start = Instant::now();
    while child.try_wait()?.is_none() {
        if !crate::session_monitor::SESSION_SAFE.load(Ordering::Relaxed)
            || start.elapsed() > Duration::from_secs(65)
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!("Operação da chave cancelada para proteger o cofre"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut output = Zeroizing::new(String::new());
    child
        .stdout
        .take()
        .unwrap()
        .take(16384)
        .read_to_string(&mut output)?;
    let result: (String,String) = serde_json::from_str(&output).map_err(|_|anyhow!("Não foi possível usar a chave. Conecte uma única chave compatível, confira o PIN e toque nela."))?;
    let secret = Zeroizing::new(hex::decode(&result.1)?);
    Ok((result.0, secret))
}
pub fn hardware_worker() -> Result<()> {
    use std::io::{Read, Write};
    let mut input = Zeroizing::new(String::new());
    std::io::stdin().take(16384).read_to_string(&mut input)?;
    let mut request: HardwareRequest = serde_json::from_str(&input)?;
    let result = hardware_native(
        request.enroll,
        &request.credential,
        &request.salt,
        &request.pin,
    );
    use zeroize::Zeroize;
    request.pin.zeroize();
    let (credential, secret) = result?;
    let output = Zeroizing::new(serde_json::to_vec(&(credential, hex::encode(&*secret)))?);
    std::io::stdout().write_all(&output)?;
    Ok(())
}
