//! Workspace organization is stored atomically inside the mounted vault.
use crate::storage;
use anyhow::{ensure, Context, Result};
use gtk::{
    gdk_pixbuf::{Colorspace, Pixbuf},
    glib,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub categories: Vec<Category>,
    pub image: Option<String>,
    pub image_revision: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub workspace_id: String,
    pub category_id: Option<String>,
    pub service: String,
}
#[derive(Serialize, Deserialize)]
pub struct Catalog {
    version: u32,
    #[serde(default)]
    last_account: Option<String>,
    pub accounts: Vec<Account>,
    pub workspaces: Vec<Workspace>,
    pub placements: BTreeMap<String, Placement>,
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn text<'a>(data: &'a Value, field: &str) -> Result<&'a str> {
    data[field].as_str().context("Required field missing")
}
fn name(data: &Value) -> Result<String> {
    let value = text(data, "name")?.trim();
    ensure!(
        !value.is_empty() && value.chars().count() <= 40 && !value.chars().any(char::is_control),
        "Use a name between 1 and 40 characters"
    );
    Ok(value.into())
}
fn new_workspace(name: String) -> Workspace {
    Workspace {
        id: id(),
        name,
        categories: vec![],
        image: None,
        image_revision: None,
    }
}
// Accept fixed-size raw pixels, not an untrusted encoded image or URL. The native
// encoder creates a metadata-free PNG which is persisted as base64 in the vault.
fn image_from_pixels(pixels: &str) -> Result<String> {
    ensure!(
        pixels.len() == 64 * 64 * 4 * 2,
        "Invalid workspace image size"
    );
    let bytes = glib::Bytes::from_owned(hex::decode(pixels)?);
    let image = Pixbuf::from_bytes(&bytes, Colorspace::Rgb, true, 8, 64, 64, 64 * 4);
    let png = image.save_to_bufferv("png", &[])?;
    Ok(format!(
        "data:image/png;base64,{}",
        glib::base64_encode(&png)
    ))
}
impl Catalog {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("catalog.json");
        if path.exists() {
            let catalog: Self = storage::read(&path)?;
            ensure!(
                catalog.version == 1,
                "Unsupported workspace catalog version"
            );
            return Ok(catalog);
        }
        let legacy = root.join("accounts.json");
        let accounts: Vec<Account> = if legacy.exists() {
            storage::read(&legacy)?
        } else {
            vec![]
        };
        let workspace = new_workspace("My workspace".into());
        let placements = accounts
            .iter()
            .map(|a| {
                (
                    a.id.clone(),
                    Placement {
                        workspace_id: workspace.id.clone(),
                        category_id: None,
                        service: "whatsapp".into(),
                    },
                )
            })
            .collect();
        let catalog = Self {
            version: 1,
            last_account: None,
            accounts,
            workspaces: vec![workspace],
            placements,
        };
        catalog.save(root)?;
        Ok(catalog)
    }
    pub fn preferred_account(&self) -> Option<&str> {
        self.last_account
            .as_deref()
            .filter(|id| self.accounts.iter().any(|a| a.id == *id))
            .or_else(|| self.accounts.first().map(|a| a.id.as_str()))
    }
    pub fn activate(&mut self, id: &str) -> Result<bool> {
        ensure!(
            self.accounts.iter().any(|account| account.id == id),
            "Account not found"
        );
        if self.last_account.as_deref() == Some(id) {
            return Ok(false);
        }
        self.last_account = Some(id.to_owned());
        Ok(true)
    }
    pub fn save(&self, root: &Path) -> Result<()> {
        storage::write(&root.join("catalog.json"), self)
    }
    // Keep image payloads out of status replies to preserve the 64 KiB IPC bound.
    pub fn metadata(&self) -> Value {
        json!({"workspaces": self.workspaces.iter().map(|w| json!({
            "id": w.id, "name": w.name, "categories": w.categories, "imageRevision": w.image_revision
        })).collect::<Vec<_>>(), "placements": self.placements})
    }
    fn workspace(&self, id: &str) -> Result<&Workspace> {
        self.workspaces
            .iter()
            .find(|w| w.id == id)
            .context("Workspace not found")
    }
    fn workspace_mut(&mut self, id: &str) -> Result<&mut Workspace> {
        self.workspaces
            .iter_mut()
            .find(|w| w.id == id)
            .context("Workspace not found")
    }
    pub fn image(&self, id: &str) -> Result<Value> {
        Ok(json!(self.workspace(id)?.image))
    }
    fn placement(&self, data: &Value) -> Result<Placement> {
        let workspace_id = text(data, "workspaceId")?;
        let workspace = self.workspace(workspace_id)?;
        let category_id: Option<String> = serde_json::from_value(data["categoryId"].clone())?;
        ensure!(
            category_id
                .as_ref()
                .is_none_or(|id| workspace.categories.iter().any(|c| &c.id == id)),
            "Category does not belong to this workspace"
        );
        Ok(Placement {
            workspace_id: workspace_id.into(),
            category_id,
            service: "whatsapp".into(),
        })
    }
    pub fn apply(&mut self, op: &str, data: &Value) -> Result<()> {
        match op {
            "workspace.add" => {
                ensure!(
                    self.workspaces.len() < 20,
                    "A maximum of twenty workspaces is supported"
                );
                let mut workspace = new_workspace(name(data)?);
                if let Some(pixels) = data.get("imagePixels") {
                    if !pixels.is_null() {
                        workspace.image = Some(image_from_pixels(
                            pixels.as_str().context("Invalid image")?,
                        )?);
                        workspace.image_revision = Some(id());
                    }
                }
                self.workspaces.push(workspace);
            }
            "workspace.update" => {
                let workspace = self.workspace_mut(text(data, "id")?)?;
                workspace.name = name(data)?;
                if let Some(pixels) = data.get("imagePixels") {
                    workspace.image = if pixels.is_null() {
                        None
                    } else {
                        Some(image_from_pixels(
                            pixels.as_str().context("Invalid image")?,
                        )?)
                    };
                    workspace.image_revision = workspace.image.as_ref().map(|_| id());
                }
            }
            "workspace.remove" => {
                let id = text(data, "id")?;
                self.workspace(id)?;
                ensure!(self.workspaces.len() > 1, "Keep at least one workspace");
                ensure!(
                    !self.placements.values().any(|p| p.workspace_id == id),
                    "Move the channels before removing this workspace"
                );
                self.workspaces.retain(|w| w.id != id);
            }
            "category.add" => {
                ensure!(
                    self.workspaces
                        .iter()
                        .map(|w| w.categories.len())
                        .sum::<usize>()
                        < 40,
                    "A maximum of forty categories is supported"
                );
                self.workspace_mut(text(data, "workspaceId")?)?
                    .categories
                    .push(Category {
                        id: id(),
                        name: name(data)?,
                    });
            }
            "category.rename" => {
                self.workspace_mut(text(data, "workspaceId")?)?
                    .categories
                    .iter_mut()
                    .find(|c| c.id == data["id"])
                    .context("Category not found")?
                    .name = name(data)?;
            }
            "category.remove" => {
                let workspace_id = text(data, "workspaceId")?;
                let category_id = text(data, "id")?;
                let workspace = self.workspace_mut(workspace_id)?;
                ensure!(
                    workspace.categories.iter().any(|c| c.id == category_id),
                    "Category not found"
                );
                workspace.categories.retain(|c| c.id != category_id);
                for placement in self.placements.values_mut().filter(|p| {
                    p.workspace_id == workspace_id && p.category_id.as_deref() == Some(category_id)
                }) {
                    placement.category_id = None;
                }
            }
            "account.add" => {
                ensure!(
                    self.accounts.len() < 20,
                    "A maximum of twenty accounts is supported"
                );
                let placement = self.placement(data)?;
                let account = Account {
                    id: id(),
                    name: name(data)?,
                };
                self.placements.insert(account.id.clone(), placement);
                self.accounts.push(account);
            }
            "account.update" => {
                let placement = self.placement(data)?;
                self.accounts
                    .iter_mut()
                    .find(|a| a.id == data["id"])
                    .context("Account not found")?
                    .name = name(data)?;
                self.placements.insert(text(data, "id")?.into(), placement);
            }
            "account.move" => {
                let placement = self.placement(data)?;
                *self
                    .placements
                    .get_mut(text(data, "id")?)
                    .context("Account not found")? = placement;
            }
            "account.rename" => {
                self.accounts
                    .iter_mut()
                    .find(|a| a.id == data["id"])
                    .context("Account not found")?
                    .name = name(data)?;
            }
            "account.remove" => {
                let id = text(data, "id")?;
                ensure!(
                    self.accounts.iter().any(|a| a.id == id),
                    "Account not found"
                );
                self.accounts.retain(|a| a.id != id);
                self.placements.remove(id);
            }
            "account.reorder" => {
                let ids: Vec<String> = serde_json::from_value(data["ids"].clone())?;
                ensure!(ids.len() == self.accounts.len(), "Invalid account order");
                let mut ordered = Vec::new();
                for id in ids {
                    ensure!(
                        !ordered.iter().any(|a: &Account| a.id == id),
                        "Duplicate account"
                    );
                    ordered.push(
                        self.accounts
                            .iter()
                            .find(|a| a.id == id)
                            .context("Account not found")?
                            .clone(),
                    );
                }
                self.accounts = ordered;
            }
            _ => anyhow::bail!("Operation not allowed"),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remembers_only_existing_accounts_and_falls_back_after_removal() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::load(dir.path()).unwrap();
        assert_eq!(catalog.preferred_account(), None);
        let workspace = catalog.workspaces[0].id.clone();
        for name in ["First", "Last"] {
            catalog
                .apply(
                    "account.add",
                    &json!({"name":name,"workspaceId":workspace,"categoryId":null}),
                )
                .unwrap();
        }
        let first = catalog.accounts[0].id.clone();
        let last = catalog.accounts[1].id.clone();
        assert!(catalog.activate("unknown").is_err());
        catalog.activate(&last).unwrap();
        catalog.save(dir.path()).unwrap();
        let mut loaded = Catalog::load(dir.path()).unwrap();
        assert_eq!(loaded.preferred_account(), Some(last.as_str()));
        loaded.apply("account.remove", &json!({"id":last})).unwrap();
        assert_eq!(loaded.preferred_account(), Some(first.as_str()));
        let mut legacy = serde_json::to_value(&loaded).unwrap();
        legacy.as_object_mut().unwrap().remove("last_account");
        let migrated: Catalog = serde_json::from_value(legacy).unwrap();
        assert_eq!(migrated.preferred_account(), Some(first.as_str()));
    }

    #[test]
    fn migration_preserves_profile_ids_and_runs_only_once() {
        let dir = tempfile::tempdir().unwrap();
        let accounts = vec![Account {
            id: id(),
            name: "Existing account".into(),
        }];
        storage::write(&dir.path().join("accounts.json"), &accounts).unwrap();
        let mut catalog = Catalog::load(dir.path()).unwrap();
        assert_eq!(catalog.accounts[0].id, accounts[0].id);
        let workspace_id = catalog.workspaces[0].id.clone();
        assert_eq!(
            catalog.placements[&accounts[0].id].workspace_id,
            workspace_id
        );
        catalog
            .apply("account.remove", &json!({"id": accounts[0].id}))
            .unwrap();
        catalog.save(dir.path()).unwrap();
        let reloaded = Catalog::load(dir.path()).unwrap();
        assert!(reloaded.accounts.is_empty());
        assert_eq!(reloaded.workspaces[0].id, workspace_id);
    }
    #[test]
    fn categories_are_scoped_moves_preserve_sessions_and_removal_keeps_channels() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::load(dir.path()).unwrap();
        let first = catalog.workspaces[0].id.clone();
        catalog
            .apply("category.add", &json!({"workspaceId":first, "name":"Team"}))
            .unwrap();
        let category = catalog.workspaces[0].categories[0].id.clone();
        catalog
            .apply(
                "account.add",
                &json!({"name":"Brazil", "workspaceId":first, "categoryId":category}),
            )
            .unwrap();
        let account = catalog.accounts[0].id.clone();
        catalog
            .apply("workspace.add", &json!({"name":"Company"}))
            .unwrap();
        let second = catalog.workspaces[1].id.clone();
        assert!(catalog
            .apply(
                "account.move",
                &json!({"id":account, "workspaceId":second, "categoryId":category})
            )
            .is_err());
        assert!(catalog
            .apply("workspace.remove", &json!({"id":first}))
            .is_err());
        catalog
            .apply(
                "category.remove",
                &json!({"workspaceId":first, "id":category}),
            )
            .unwrap();
        assert!(catalog.placements[&account].category_id.is_none());
        catalog
            .apply("account.move", &json!({"id":account, "workspaceId":second}))
            .unwrap();
        catalog
            .apply("workspace.remove", &json!({"id":first}))
            .unwrap();
        catalog.save(dir.path()).unwrap();
        let reloaded = Catalog::load(dir.path()).unwrap();
        assert_eq!(reloaded.accounts[0].id, account);
        assert_eq!(reloaded.placements[&account].workspace_id, second);
        assert_eq!(reloaded.placements[&account].service, "whatsapp");
    }
    #[test]
    fn images_are_native_encoded_base64_and_do_not_inflate_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::load(dir.path()).unwrap();
        let first = catalog.workspaces[0].id.clone();
        for invalid in [
            "https://example.com/image",
            "data:image/svg+xml,<svg/>",
            "00",
        ] {
            assert!(catalog
                .apply(
                    "workspace.update",
                    &json!({"id":first,"name":"One","imagePixels":invalid})
                )
                .is_err());
        }
        let pixels = "aabbccff".repeat(64 * 64);
        catalog
            .apply(
                "workspace.update",
                &json!({"id":first,"name":"One","imagePixels":pixels}),
            )
            .unwrap();
        let image = catalog.image(&first).unwrap();
        let source = image.as_str().unwrap();
        assert!(source.starts_with("data:image/png;base64,"));
        let png = glib::base64_decode(source.strip_prefix("data:image/png;base64,").unwrap());
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(!catalog.metadata().to_string().contains("base64"));
        assert!(serde_json::to_vec(&image).unwrap().len() < 64 * 1024);
        catalog.save(dir.path()).unwrap();
        assert_eq!(
            Catalog::load(dir.path()).unwrap().image(&first).unwrap(),
            image
        );
        catalog
            .apply(
                "workspace.update",
                &json!({"id":first,"name":"One","imagePixels":null}),
            )
            .unwrap();
        assert!(catalog.image(&first).unwrap().is_null());
    }
}
