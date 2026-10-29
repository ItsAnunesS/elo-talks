use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, ensure, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}
pub fn random() -> Zeroizing<Vec<u8>> {
    let mut b = Zeroizing::new(vec![0; 32]);
    OsRng.fill_bytes(&mut b);
    b
}
fn derive(secret: &[u8], salt: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let p = Params::new(65536, 3, 1, Some(32)).map_err(|_| anyhow!("Parâmetros inválidos"))?;
    let mut key = Zeroizing::new(vec![0; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, p)
        .hash_password_into(secret, salt, &mut key)
        .map_err(|_| anyhow!("Falha ao derivar chave"))?;
    Ok(key)
}
pub fn wrap(master: &[u8], secret: &[u8], context: &str) -> Result<Envelope> {
    let salt = random();
    let mut nonce = [0; 12];
    OsRng.fill_bytes(&mut nonce);
    let key = derive(secret, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| anyhow!("Chave inválida"))?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: master,
                aad: context.as_bytes(),
            },
        )
        .map_err(|_| anyhow!("Falha ao proteger chave"))?;
    Ok(Envelope {
        salt: hex::encode(&*salt),
        nonce: hex::encode(nonce),
        ciphertext: hex::encode(ciphertext),
    })
}
pub fn unwrap(e: &Envelope, secret: &[u8], context: &str) -> Result<Zeroizing<Vec<u8>>> {
    let salt = hex::decode(&e.salt)?;
    let nonce = hex::decode(&e.nonce)?;
    ensure!(
        salt.len() == 32 && nonce.len() == 12 && e.ciphertext.len() == 96,
        "Cofre inválido"
    );
    let key = derive(secret, &salt)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| anyhow!("Chave inválida"))?;
    let plain = cipher
        .decrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: &hex::decode(&e.ciphertext)?,
                aad: context.as_bytes(),
            },
        )
        .map_err(|_| anyhow!("Credencial incorreta ou cofre alterado"))?;
    ensure!(plain.len() == 32, "Chave inválida");
    Ok(Zeroizing::new(plain))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authentication_and_domain_separation() {
        let m = random();
        let mut e = wrap(&m, b"long password", "vault:password").unwrap();
        assert_eq!(*unwrap(&e, b"long password", "vault:password").unwrap(), *m);
        assert!(unwrap(&e, b"wrong", "vault:password").is_err());
        assert!(unwrap(&e, b"long password", "vault:pin").is_err());
        e.ciphertext.replace_range(..2, "ff");
        assert!(unwrap(&e, b"long password", "vault:password").is_err());
    }
}

#[cfg(test)]
mod glib_regression {
    use gtk::glib::prelude::ToVariant;
    #[test]
    fn string_variant_iterators_use_valid_out_pointer() {
        let v = ["one", "two", "three"].to_variant();
        assert_eq!(
            v.array_iter_str().unwrap().collect::<Vec<_>>(),
            ["one", "two", "three"]
        );
        assert_eq!(
            v.array_iter_str().unwrap().rev().collect::<Vec<_>>(),
            ["three", "two", "one"]
        );
        assert_eq!(v.array_iter_str().unwrap().nth(1), Some("two"));
        assert_eq!(v.array_iter_str().unwrap().nth_back(1), Some("two"));
    }
}
