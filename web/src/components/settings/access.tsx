import { useState } from "react";

import type { SettingsDocument } from "@/lib/settings";

import { hasConfigAccess } from "@/lib/settings";

import { EntryEditor, EntryList, RemoveEntry } from "./entry-list";
import { type EditorProps, useSettingsForm } from "./form";
import { clientSchema } from "./schemas";

export function Access(props: EditorProps) {
  const [selected, setSelected] = useState<string | null>(null);
  const select = (value: string | null) => {
    props.onEdit();
    setSelected(value);
  };
  const clients = props.document.settings.clients;
  const client = clients.find((entry) => entry.name === selected);

  if (selected === "" || client) {
    return (
      <EntryEditor
        title={client ? `Edit ${client.name}` : "Add client"}
        onBack={() => select(null)}
      >
        <ClientEditor
          key={props.document.revision}
          {...props}
          onSaved={(restart) => {
            props.onSaved(restart);

            if (selected === "") {
              setSelected(null);
            }
          }}
          client={client}
          onRemoved={(restart) => {
            props.onSaved(restart);
            setSelected(null);
          }}
        />
      </EntryEditor>
    );
  }

  return (
    <EntryList
      message={props.message}
      title="Client access"
      addLabel="Add client"
      onAdd={() => select("")}
      onSelect={select}
      entries={clients.map((entry) => ({
        detail: entry.permissions.join(", "),
        id: entry.name,
        name: entry.name,
      }))}
    />
  );
}

function ClientEditor({
  client,
  onRemoved,
  ...props
}: {
  client?: SettingsDocument["clients"][number];
  onRemoved: EditorProps["onSaved"];
} & EditorProps) {
  const { settings } = props.document;
  const oauthBindings =
    settings.oauth?.bindings.filter(
      (binding) => binding.client === client?.name,
    ).length ?? 0;
  let removalBlocked: string | undefined;

  if (oauthBindings > 0) {
    const identityLabel = oauthBindings === 1 ? "identity" : "identities";

    removalBlocked = `Used by ${String(oauthBindings)} OAuth ${identityLabel}. Remove these bindings in OAuth settings first.`;
  }

  if (
    client &&
    hasConfigAccess(client.permissions) &&
    !settings.clients.some(
      (entry) =>
        entry.name !== client.name && hasConfigAccess(entry.permissions),
    )
  ) {
    removalBlocked = "Keep at least one client with configuration access.";
  }

  const { form, mutation } = useSettingsForm(
    props,
    {
      key: "",
      name: client?.name ?? "",
      permissions: client?.permissions.join(", ") ?? "",
      plex_id: client?.seerr_user?.plex_id.toString() ?? "",
      seerr_id: client?.seerr_user?.id.toString() ?? "",
    },
    clientSchema(
      settings.clients
        .filter((entry) => entry.name !== client?.name)
        .map((entry) => entry.name),
      Boolean(client),
      oauthBindings > 0,
    ),
    (value) => {
      const updated = {
        key: value.key || null,
        name: value.name,
        permissions: value.permissions
          .split(",")
          .map((entry) => entry.trim())
          .filter(Boolean),
        seerr_user: value.seerr_id
          ? { id: Number(value.seerr_id), plex_id: Number(value.plex_id) }
          : null,
      };

      return {
        ...settings,
        clients: client
          ? settings.clients.map((entry) =>
              entry.name === client.name ? updated : entry,
            )
          : [...settings.clients, updated],
      };
    },
  );

  return (
    <>
      <form.AppForm>
        <form.Editor mutation={mutation} message={props.message}>
          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="name">
              {(field) => (
                <field.Text label="Name" readOnly={Boolean(client)} />
              )}
            </form.AppField>
            <form.AppField name="key">
              {(field) => (
                <field.Text
                  label="Client key"
                  type="password"
                  placeholder={client ? "Unchanged" : "At least 32 characters"}
                />
              )}
            </form.AppField>
            <form.AppField name="seerr_id">
              {(field) => (
                <field.Text label="Seerr user ID" type="number" min={1} />
              )}
            </form.AppField>
            <form.AppField name="plex_id">
              {(field) => (
                <field.Text label="Plex user ID" type="number" min={1} />
              )}
            </form.AppField>
          </div>
          <form.AppField name="permissions">
            {(field) => <field.Text label="Permissions" />}
          </form.AppField>
          <p className="text-xs text-muted-foreground">
            Separate permissions with commas. Use source:endpoint for native
            reads, admin for full access, or catalog:view, requests:view and
            requests:create for a request account. Set both user IDs to bind a
            Plex-linked Seerr account.
          </p>
        </form.Editor>
      </form.AppForm>
      {client && (
        <RemoveEntry
          {...props}
          onSaved={onRemoved}
          label={client.name}
          hint="Removing the connected client signs you out."
          blocked={removalBlocked}
          settings={{
            ...settings,
            clients: settings.clients.filter(
              (entry) => entry.name !== client.name,
            ),
          }}
        />
      )}
    </>
  );
}
