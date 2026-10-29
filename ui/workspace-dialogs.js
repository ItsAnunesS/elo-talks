import {
  createButton,
  createElement,
  createField,
  getElement,
  createDestructiveAction,
} from "./dom.js";
import { request } from "./bridge.js";
import { workspaceImagePixels } from "./workspace-image.js";

export function placementFields(state, parent, workspaceId, categoryId = null) {
  const select = (label) => {
    const caption = createElement("label", "", label);
    const input = createElement("select");
    input.id = `field-${crypto.randomUUID()}`;
    caption.htmlFor = input.id;
    parent.append(caption, input);
    return input;
  };
  const workspace = select("Workspace");
  const category = select("Category");
  const option = (name, value) => {
    const item = createElement("option", "", name);
    item.value = value;
    return item;
  };
  workspace.replaceChildren(...state.organization.workspaces.map((w) => option(w.name, w.id)));
  workspace.value = workspaceId;
  const update = () => {
    const selected = state.organization.workspaces.find((w) => w.id === workspace.value);
    category.replaceChildren(
      option("No category", ""),
      ...selected.categories.map((c) => option(c.name, c.id)),
    );
  };
  update();
  category.value = categoryId || "";
  workspace.addEventListener("change", update);
  return () => ({ workspaceId: workspace.value, categoryId: category.value || null });
}
export function workspaceDialogs({ state, active, open, close, refresh, created }) {
  async function editWorkspace(workspace) {
    let name,
      pixels,
      imageTask = Promise.resolve();
    await open(
      workspace ? "Manage workspace" : "Create workspace",
      (parent) => {
        name = createField(
          parent,
          "Workspace name",
          "text",
          workspace?.name || "",
          "E.g. André, My company",
        );
        name.maxLength = 40;
        name.required = true;
        const preview = createElement("img", "workspace-image-preview");
        preview.alt = "Workspace image preview";
        preview.hidden = true;
        if (workspace?.imageRevision)
          request("workspace.image", { id: workspace.id })
            .then((image) => {
              if (preview.isConnected && pixels === undefined && image) {
                preview.src = image;
                preview.hidden = false;
              }
            })
            .catch(() => {});
        const upload = createField(parent, "Workspace image", "file");
        upload.accept = "image/png,image/jpeg,image/webp";
        parent.append(
          preview,
          createElement(
            "p",
            "hint",
            "PNG, JPEG or WebP · up to 5 MB and 4096 × 4096 pixels. Cropped to a square and stored inside your vault.",
          ),
        );
        let sequence = 0;
        let removeImage;
        upload.addEventListener("change", () => {
          const captured = ++sequence;
          imageTask = workspaceImagePixels(upload.files[0]).then((result) => {
            if (captured !== sequence || !preview.isConnected) return;
            pixels = result.pixels;
            removeImage.hidden = false;
            preview.src = result.preview;
            preview.hidden = false;
            getElement("dialog-error").textContent = "";
          });
          imageTask.catch((error) => {
            if (captured === sequence && preview.isConnected)
              getElement("dialog-error").textContent = error.message;
          });
        });
        removeImage = createButton(
          "Remove image",
          () => {
            sequence++;
            imageTask = Promise.resolve();
            pixels = null;
            upload.value = "";
            preview.removeAttribute("src");
            preview.hidden = true;
            removeImage.hidden = true;
          },
          "text-button",
        );
        removeImage.hidden = !workspace?.imageRevision;
        parent.append(removeImage);
        if (workspace) {
          const occupied = Object.values(state().organization.placements).some(
            (p) => p.workspaceId === workspace.id,
          );
          const remove = createDestructiveAction(
            "Delete workspace",
            `Delete “${workspace.name}”? Its custom image and empty categories will be removed.`,
            async () => {
              await request("workspace.remove", { id: workspace.id });
              await close();
              await refresh();
            },
          );
          remove.querySelector("button").disabled =
            occupied || state().organization.workspaces.length === 1;
          parent.append(
            createElement(
              "p",
              "hint",
              "Move all channels out before deleting a workspace. Keep at least one workspace.",
            ),
            remove,
          );
        }
      },
      async () => {
        await imageTask;
        const data = { name: name.value };
        if (workspace) data.id = workspace.id;
        if (pixels !== undefined) data.imagePixels = pixels;
        const result = await request(workspace ? "workspace.update" : "workspace.add", data);
        if (!workspace) created(result.organization.workspaces.at(-1).id);
      },
    );
  }
  async function editCategory(category) {
    let name;
    const workspaceId = active();
    await open(
      category ? "Manage category" : "Create category",
      (parent) => {
        name = createField(parent, "Category name", "text", category?.name || "");
        name.maxLength = 40;
        name.required = true;
        if (category)
          parent.append(
            createElement(
              "p",
              "hint",
              "Deleting a category keeps its channels in the workspace without a category.",
            ),
            createButton(
              "Delete category",
              async () => {
                await request("category.remove", { workspaceId, id: category.id });
                await refresh();
                await close();
              },
              "secondary danger",
            ),
          );
      },
      () =>
        request(category ? "category.rename" : "category.add", {
          workspaceId,
          id: category?.id,
          name: name.value,
        }),
    );
  }
  return { editWorkspace, editCategory };
}
