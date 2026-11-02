//! Server-authoritative metadata only. WhatsApp profiles never cross this boundary.
use crate::workspaces::{Account, Catalog, Category, Placement, Workspace};
use anyhow::{anyhow, ensure, Context, Result};
use reqwest::{
    blocking::{Client, Response},
    header, StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    io::Read,
    sync::mpsc,
    time::{Duration, Instant},
};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const RESPONSE_LIMIT: u64 = 128 * 1024;
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Connection {
    pub origin: String,
    pub username: String,
    pub token: String,
    pub certificate: Option<String>,
    pub organization_id: Option<String>,
    pub managed_workspaces: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub role: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RemoteCategory {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RemoteChannel {
    pub id: String,
    pub name: String,
    pub category_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    pub categories: Vec<RemoteCategory>,
    pub channels: Vec<RemoteChannel>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RemoteWorkspace {
    pub id: String,
    pub organization_id: String,
    pub revision: i64,
    pub can_manage: bool,
    pub document: Document,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub organization: Organization,
    pub workspaces: Vec<RemoteWorkspace>,
}
#[derive(Deserialize, Zeroize, ZeroizeOnDrop)]
struct Login {
    access_token: String,
}
pub struct Outcome {
    pub connection: Connection,
    pub organizations: Vec<Organization>,
    pub snapshot: Option<Snapshot>,
    pub etag: Option<String>,
}
pub struct Runtime {
    pub receiver: Option<mpsc::Receiver<Result<Outcome>>>,
    pub verified: bool,
    pub status: Value,
    pub next_check: Instant,
    pub etag: Option<String>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            receiver: None,
            verified: false,
            status: json!({"phase":"local"}),
            next_check: Instant::now(),
            etag: None,
        }
    }
}
impl Runtime {
    pub fn start(&mut self, connection: Connection, etag: Option<String>) -> Result<()> {
        ensure!(
            self.receiver.is_none(),
            "A server operation is already running"
        );
        self.spawn(move || synchronize(connection, etag, false))
    }
    pub fn connect(&mut self, body: &Value, previous: Option<&Connection>) -> Result<()> {
        ensure!(
            self.receiver.is_none(),
            "A server operation is already running"
        );
        let origin = normalize_origin(
            body["domain"]
                .as_str()
                .context("Enter the server HTTPS domain")?,
        )?;
        let username = body["username"]
            .as_str()
            .context("Enter your server username")?
            .to_owned();
        ensure!(
            !username.is_empty() && username.len() <= 64,
            "Invalid username"
        );
        let password = Zeroizing::new(
            body["password"]
                .as_str()
                .context("Enter your server password")?
                .to_owned(),
        );
        ensure!(
            !password.is_empty() && password.len() <= 1024,
            "Invalid password length"
        );
        let certificate = body["certificate"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| {
                previous
                    .filter(|connection| connection.origin == origin)
                    .and_then(|connection| connection.certificate.clone())
            });
        ensure!(
            certificate
                .as_ref()
                .is_none_or(|value| value.len() <= 64 * 1024),
            "Certificate is too large"
        );
        let organization_id = previous
            .filter(|connection| connection.origin == origin && connection.username == username)
            .and_then(|connection| connection.organization_id.clone());
        let managed_workspaces = previous
            .map(|connection| connection.managed_workspaces.clone())
            .unwrap_or_default();
        self.spawn(move || {
            let mut connection = Connection { origin, username, token: String::new(), certificate, organization_id, managed_workspaces };
            let client = client(&connection)?;
            let response = client.post(format!("{}/api/v1/auth/login", connection.origin)).json(&json!({"username":connection.username,"password":&*password})).send().map_err(|_| anyhow!("Could not securely connect to the server. Check its domain, certificate and availability."))?;
            ensure!(response.status() != StatusCode::UNAUTHORIZED, "Incorrect server username or password, or the account is disabled.");
            let login: Login = decode(response)?;
            let token = &login.access_token;
            ensure!(token.len() == 64 && token.bytes().all(|value| value.is_ascii_hexdigit()), "Unsupported server token");
            connection.token = token.into();
            match synchronize(connection.clone(), None, true) {
                Ok(outcome) => Ok(outcome),
                Err(error) => { sign_out(connection); Err(error) }
            }
        })
    }
    fn spawn(&mut self, task: impl FnOnce() -> Result<Outcome> + Send + 'static) -> Result<()> {
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("server-metadata-sync".into())
            .spawn(move || {
                let _ = sender.send(task());
            })?;
        self.receiver = Some(receiver);
        self.status["phase"] = json!("syncing");
        self.status["error"] = Value::Null;
        Ok(())
    }
    pub fn take_result(&mut self) -> Option<Result<Outcome>> {
        let result = match self.receiver.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(_) => Err(anyhow!(
                "Server synchronization stopped. Retry the connection."
            )),
        };
        self.receiver = None;
        Some(result)
    }
}
pub fn normalize_origin(value: &str) -> Result<String> {
    ensure!(value.len() <= 2048, "Server domain is too long");
    let value = value.trim();
    let with_scheme = if value.contains("://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    let url = reqwest::Url::parse(&with_scheme).context("Invalid server domain")?;
    ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "Use an HTTPS domain without paths, credentials or query parameters"
    );
    Ok(url.origin().ascii_serialization())
}
/// Revocation is best-effort when offline; never block vault/session monitoring.
pub fn sign_out(connection: Connection) {
    let _ = std::thread::Builder::new()
        .name("server-session-revocation".into())
        .spawn(move || {
            if let Ok(client) = client(&connection) {
                let _ = client
                    .post(format!("{}/api/v1/auth/logout", connection.origin))
                    .bearer_auth(&connection.token)
                    .timeout(Duration::from_secs(3))
                    .send();
            }
        });
}
fn client(connection: &Connection) -> Result<Client> {
    let mut builder = Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("Elo-Talks/", env!("CARGO_PKG_VERSION")));
    if let Some(pem) = &connection.certificate {
        builder = builder.add_root_certificate(
            reqwest::Certificate::from_pem(pem.as_bytes())
                .context("Invalid trusted PEM certificate")?,
        );
    }
    builder
        .build()
        .context("Could not initialize the secure server client")
}
fn decode<T: serde::de::DeserializeOwned>(response: Response) -> Result<T> {
    let status = response.status();
    ensure!(
        status.is_success(),
        match status {
            StatusCode::UNAUTHORIZED => "Server session expired or was revoked. Sign in again.",
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND =>
                "Server access was removed or this organization is unavailable.",
            StatusCode::TOO_MANY_REQUESTS =>
                "Too many server authentication attempts. Wait before signing in again.",
            _ => "The server could not complete this operation. Retry when it is available.",
        }
    );
    let mut bytes = Zeroizing::new(Vec::new());
    response.take(RESPONSE_LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= RESPONSE_LIMIT,
        "Server response exceeds the supported limit"
    );
    serde_json::from_slice(&bytes).context("Unsupported server response")
}
fn synchronize(
    mut connection: Connection,
    etag: Option<String>,
    allow_reselection: bool,
) -> Result<Outcome> {
    let client = client(&connection)?;
    let server: Value = decode(client.get(format!("{}/api/v1/server", connection.origin)).bearer_auth(&connection.token).send().map_err(|_| anyhow!("Server connection failed. Managed workspaces are blocked until access can be verified."))?)?;
    ensure!(
        server["api_version"] == "v1",
        "Unsupported server API version"
    );
    let organizations: Vec<Organization> = decode(
        client
            .get(format!("{}/api/v1/organizations", connection.origin))
            .bearer_auth(&connection.token)
            .send()
            .map_err(|_| anyhow!("Could not refresh server organizations"))?,
    )?;
    ensure!(organizations.len() <= 100, "Too many server organizations");
    for org in &organizations {
        validate_organization(org)?;
    }
    if allow_reselection
        && connection.organization_id.as_ref().is_some_and(|id| {
            !organizations
                .iter()
                .any(|organization| &organization.id == id)
        })
    {
        connection.organization_id = None;
    }
    if connection.organization_id.is_none() && organizations.len() == 1 {
        connection.organization_id = Some(organizations[0].id.clone());
    }
    let Some(org) = &connection.organization_id else {
        return Ok(Outcome {
            connection,
            organizations,
            snapshot: None,
            etag: None,
        });
    };
    ensure!(organizations.iter().any(|value| &value.id == org), "Your access to the selected organization was removed. Choose another organization or sign in again.");
    let mut request = client
        .get(format!("{}/api/v1/sync", connection.origin))
        .query(&[("organization_id", org)])
        .bearer_auth(&connection.token);
    if let Some(etag) = &etag {
        request = request.header(header::IF_NONE_MATCH, etag);
    }
    let response = request.send().map_err(|_| anyhow!("Could not verify workspace access. Managed workspaces are blocked until the server responds."))?;
    if response.status() == StatusCode::NOT_MODIFIED {
        return Ok(Outcome {
            connection,
            organizations,
            snapshot: None,
            etag,
        });
    }
    let new_etag = response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() <= 100)
        .map(str::to_owned);
    let snapshot: Snapshot = decode(response)?;
    validate_snapshot(&snapshot, org)?;
    Ok(Outcome {
        connection,
        organizations,
        snapshot: Some(snapshot),
        etag: new_etag,
    })
}
fn validate_name(value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty()
            && value.chars().count() <= 40
            && !value.chars().any(char::is_control),
        "Invalid server name"
    );
    Ok(())
}
fn validate_organization(org: &Organization) -> Result<()> {
    Uuid::parse_str(&org.id)?;
    validate_name(&org.name)?;
    ensure!(
        matches!(
            org.role.as_str(),
            "server_admin" | "organization_admin" | "manager" | "member"
        ),
        "Unsupported server role"
    );
    Ok(())
}
pub fn validate_snapshot(snapshot: &Snapshot, organization: &str) -> Result<()> {
    validate_organization(&snapshot.organization)?;
    ensure!(
        snapshot.organization.id == organization && snapshot.workspaces.len() <= 20,
        "Invalid organization snapshot"
    );
    let mut ids = HashSet::new();
    let mut channel_count = 0;
    let mut category_count = 0;
    for workspace in &snapshot.workspaces {
        Uuid::parse_str(&workspace.id)?;
        ensure!(
            ids.insert(&workspace.id)
                && workspace.organization_id == organization
                && workspace.revision > 0,
            "Invalid workspace snapshot"
        );
        validate_name(&workspace.document.name)?;
        let mut category_ids = HashSet::new();
        for category in &workspace.document.categories {
            Uuid::parse_str(&category.id)?;
            validate_name(&category.name)?;
            ensure!(
                category_ids.insert(&category.id),
                "Duplicate server category"
            );
        }
        let mut channel_ids = HashSet::new();
        for channel in &workspace.document.channels {
            Uuid::parse_str(&channel.id)?;
            validate_name(&channel.name)?;
            ensure!(
                channel_ids.insert(&channel.id)
                    && channel
                        .category_id
                        .as_ref()
                        .is_none_or(|id| category_ids.contains(id)),
                "Invalid server channel"
            );
        }
        channel_count += workspace.document.channels.len();
        category_count += workspace.document.categories.len();
    }
    ensure!(
        channel_count <= 20 && category_count <= 40,
        "Server snapshot exceeds desktop limits"
    );
    Ok(())
}
fn mapped_id(connection: &Connection, workspace: &str, kind: &str, id: &str) -> String {
    let hash = Sha256::digest(format!(
        "elo:server:v1|{}|{}|{}|{workspace}|{kind}|{id}",
        connection.origin,
        connection.username,
        connection.organization_id.as_deref().unwrap_or_default()
    ));
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes).to_string()
}
pub fn remove_managed(catalog: &mut Catalog) {
    let ids = catalog
        .server
        .as_ref()
        .map(|connection| connection.managed_workspaces.clone())
        .unwrap_or_default();
    catalog
        .workspaces
        .retain(|workspace| !ids.contains(&workspace.id));
    catalog.accounts.retain(|account| {
        !catalog
            .placements
            .get(&account.id)
            .is_some_and(|placement| ids.contains(&placement.workspace_id))
    });
    catalog
        .placements
        .retain(|_, placement| !ids.contains(&placement.workspace_id));
}
pub fn apply_snapshot(
    catalog: &mut Catalog,
    connection: &mut Connection,
    snapshot: Option<&Snapshot>,
) -> Result<()> {
    remove_managed(catalog);
    connection.managed_workspaces.clear();
    if let Some(snapshot) = snapshot {
        validate_snapshot(
            snapshot,
            connection
                .organization_id
                .as_deref()
                .context("Select an organization")?,
        )?;
        for remote in &snapshot.workspaces {
            let id = mapped_id(connection, &remote.id, "workspace", &remote.id);
            let categories = remote
                .document
                .categories
                .iter()
                .map(|category| Category {
                    id: mapped_id(connection, &remote.id, "category", &category.id),
                    name: category.name.clone(),
                })
                .collect();
            catalog.workspaces.push(Workspace {
                id: id.clone(),
                name: remote.document.name.clone(),
                categories,
                image: None,
                image_revision: None,
            });
            for channel in &remote.document.channels {
                let channel_id = mapped_id(connection, &remote.id, "channel", &channel.id);
                catalog.accounts.push(Account {
                    id: channel_id.clone(),
                    name: channel.name.clone(),
                });
                catalog.placements.insert(
                    channel_id,
                    Placement {
                        workspace_id: id.clone(),
                        category_id: channel.category_id.as_ref().map(|category| {
                            mapped_id(connection, &remote.id, "category", category)
                        }),
                        service: "whatsapp".into(),
                    },
                );
            }
            connection.managed_workspaces.push(id);
        }
    }
    if catalog.workspaces.is_empty() {
        catalog.apply("workspace.add", &json!({"name":"My workspace"}))?;
    }
    ensure!(catalog.workspaces.len() <= 20 && catalog.accounts.len() <= 20 && catalog.workspaces.iter().map(|workspace| workspace.categories.len()).sum::<usize>() <= 40, "Local and server workspaces exceed desktop limits (20 workspaces, 20 channels, 40 categories). Reduce local content before syncing.");
    catalog.server = Some(connection.clone());
    Ok(())
}
pub fn managed_mutation(catalog: &Catalog, operation: &str, data: &Value) -> bool {
    let Some(connection) = &catalog.server else {
        return false;
    };
    let managed = &connection.managed_workspaces;
    let source = data["id"]
        .as_str()
        .and_then(|id| catalog.placements.get(id))
        .is_some_and(|placement| managed.contains(&placement.workspace_id));
    source
        || data["workspaceId"]
            .as_str()
            .is_some_and(|id| managed.iter().any(|value| value == id))
        || (operation.starts_with("workspace.")
            && data["id"]
                .as_str()
                .is_some_and(|id| managed.iter().any(|value| value == id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn connection() -> Connection {
        Connection {
            origin: "https://server.example".into(),
            username: "alice".into(),
            token: "a".repeat(64),
            certificate: None,
            organization_id: Some(Uuid::new_v4().to_string()),
            managed_workspaces: vec![],
        }
    }
    fn snapshot(connection: &Connection) -> Snapshot {
        let category = Uuid::new_v4().to_string();
        Snapshot {
            organization: Organization {
                id: connection.organization_id.clone().unwrap(),
                name: "Team".into(),
                role: "member".into(),
            },
            workspaces: vec![RemoteWorkspace {
                id: Uuid::new_v4().to_string(),
                organization_id: connection.organization_id.clone().unwrap(),
                revision: 1,
                can_manage: false,
                document: Document {
                    name: "Shared".into(),
                    categories: vec![RemoteCategory {
                        id: category.clone(),
                        name: "Support".into(),
                    }],
                    channels: vec![RemoteChannel {
                        id: Uuid::new_v4().to_string(),
                        name: "Service".into(),
                        category_id: Some(category),
                    }],
                },
            }],
        }
    }
    #[test]
    fn metadata_sync_is_idempotent_preserves_local_data_and_never_exposes_tokens() {
        let root = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::load(root.path()).unwrap();
        let local_id = catalog.workspaces[0].id.clone();
        catalog
            .apply(
                "account.add",
                &json!({"name":"Local","workspaceId":local_id,"categoryId":null}),
            )
            .unwrap();
        let local_account = catalog.accounts[0].id.clone();
        let mut connection = connection();
        let mut snapshot = snapshot(&connection);
        apply_snapshot(&mut catalog, &mut connection, Some(&snapshot)).unwrap();
        let first = catalog.accounts[1].id.clone();
        catalog.save(root.path()).unwrap();
        let restored = Catalog::load(root.path()).unwrap();
        assert!(restored.server.is_some());
        assert!(!restored.metadata().to_string().contains(&connection.token));
        apply_snapshot(&mut catalog, &mut connection, Some(&snapshot)).unwrap();
        assert_eq!(catalog.accounts.len(), 2);
        assert_eq!(catalog.accounts[1].id, first);
        assert!(managed_mutation(
            &catalog,
            "account.remove",
            &json!({"id":first})
        ));
        snapshot.workspaces.clear();
        apply_snapshot(&mut catalog, &mut connection, Some(&snapshot)).unwrap();
        assert_eq!(catalog.accounts.len(), 1);
        assert_eq!(catalog.accounts[0].id, local_account);
        assert_eq!(catalog.workspaces[0].id, local_id);
    }
    #[test]
    fn profiles_are_namespaced_by_domain_user_organization_and_workspace() {
        let mut connection = connection();
        let first = mapped_id(&connection, "workspace", "channel", "same-id");
        connection.username = "bob".into();
        assert_ne!(
            first,
            mapped_id(&connection, "workspace", "channel", "same-id")
        );
        connection.username = "alice".into();
        connection.origin = "https://other.example".into();
        assert_ne!(
            first,
            mapped_id(&connection, "workspace", "channel", "same-id")
        );
        connection.origin = "https://server.example".into();
        assert_ne!(
            first,
            mapped_id(&connection, "other-workspace", "channel", "same-id")
        );
        connection.organization_id = Some(Uuid::new_v4().to_string());
        assert_ne!(
            first,
            mapped_id(&connection, "workspace", "channel", "same-id")
        );
    }
    #[test]
    fn unsafe_origins_and_cross_organization_payloads_are_rejected() {
        for value in [
            "http://example.com",
            "https://user:secret@example.com",
            "https://example.com/path",
            "https://example.com?token=x",
            "https://example.com/#x",
        ] {
            assert!(normalize_origin(value).is_err());
        }
        assert_eq!(
            normalize_origin("Example.COM/").unwrap(),
            "https://example.com"
        );
        let connection = connection();
        let mut snapshot = snapshot(&connection);
        snapshot.workspaces[0].organization_id = Uuid::new_v4().to_string();
        assert!(
            validate_snapshot(&snapshot, connection.organization_id.as_ref().unwrap()).is_err()
        );
    }
    #[test]
    #[ignore = "requires a disposable PostgreSQL-backed HTTPS service; see docs/SERVER-SYNC.md"]
    fn syncs_with_real_https_postgres_service() {
        let origin = std::env::var("ELO_SYNC_TEST_ORIGIN").unwrap();
        let username = std::env::var("ELO_SYNC_TEST_USERNAME").unwrap();
        let password = Zeroizing::new(std::env::var("ELO_SYNC_TEST_PASSWORD").unwrap());
        let certificate =
            std::fs::read_to_string(std::env::var("ELO_SYNC_TEST_CA").unwrap()).unwrap();
        let mut runtime = Runtime::default();
        runtime.connect(&json!({"domain":origin,"username":username,"password":&*password,"certificate":certificate}),None).unwrap();
        let mut outcome = runtime
            .receiver
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        if outcome.snapshot.is_none() {
            outcome.connection.organization_id =
                Some(std::env::var("ELO_SYNC_TEST_ORGANIZATION").unwrap());
            outcome = synchronize(outcome.connection, None, false).unwrap();
        }
        assert!(outcome.snapshot.is_some());
        let root = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::load(root.path()).unwrap();
        apply_snapshot(
            &mut catalog,
            &mut outcome.connection,
            outcome.snapshot.as_ref(),
        )
        .unwrap();
        catalog.save(root.path()).unwrap();
        assert!(!outcome.connection.managed_workspaces.is_empty());
        let again = synchronize(outcome.connection.clone(), outcome.etag.clone(), false).unwrap();
        assert!(again.snapshot.is_none());
        let selected = outcome.connection.organization_id.clone();
        let previous_ids = catalog
            .accounts
            .iter()
            .map(|account| account.id.clone())
            .collect::<Vec<_>>();
        let mut reconnected = Runtime::default();
        reconnected.connect(&json!({"domain":origin,"username":username,"password":&*password,"certificate":certificate}), Some(&outcome.connection)).unwrap();
        let mut renewed = reconnected
            .receiver
            .take()
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(renewed.connection.organization_id, selected);
        assert!(renewed.snapshot.is_some());
        apply_snapshot(
            &mut catalog,
            &mut renewed.connection,
            renewed.snapshot.as_ref(),
        )
        .unwrap();
        assert_eq!(
            catalog
                .accounts
                .iter()
                .map(|account| account.id.clone())
                .collect::<Vec<_>>(),
            previous_ids
        );
    }
}
