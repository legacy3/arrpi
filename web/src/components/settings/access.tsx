import { KeyRound } from "lucide-react";
import { parseAsString, useQueryState } from "nuqs";

import type { SettingsDocument } from "@/lib/settings";

import { CopyText } from "@/components/copy-text";
import { Button } from "@/components/ui/button";
import { hasConfigAccess } from "@/lib/settings";

import { AccountPicker } from "./account-picker";
import { EntryEditor, EntryList, RemoveEntry } from "./entry-list";
import { type EditorProps, useSettingsForm } from "./form";
import { Grants } from "./grants";
import { clientSchema } from "./schemas";

export function Access(props: EditorProps) {
  const [entry, setEntry] = useQueryState("entry", parseAsString);
  const selected = entry === "__new" ? "" : entry;
  const setSelected = (value: string | null) =>
    void setEntry(value === "" ? "__new" : value);
  const select = (value: string | null) => {
    props.onEdit();
    setSelected(value);
  };
  const clients = props.document.settings.clients;
  const client = clients.find((entry) => entry.name === selected);

  if (selected === "" || client) {
    return (
      <EntryEditor title={client ? `Edit ${client.name}` : "Add client"}>
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
        detail: `${entry.permissions.length} grants${entry.seerr_user ? ", Plex-linked requester" : ""}${hasConfigAccess(entry.permissions) ? ", configuration access" : ""}`,
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
        ...client,
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
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => {
                const bytes = crypto.getRandomValues(new Uint8Array(48));

                form.setFieldValue(
                  "key",
                  Array.from(bytes, (value) =>
                    value.toString(16).padStart(2, "0"),
                  ).join(""),
                );
              }}
            >
              <KeyRound className="size-4" />
              Generate key
            </Button>
            <form.Subscribe selector={(state) => state.values.key}>
              {(key) => <CopyText key={key} value={key} label="Copy new key" />}
            </form.Subscribe>
          </div>
          <form.Subscribe
            selector={(state) => ({
              id: state.values.seerr_id,
              plexId: state.values.plex_id,
            })}
          >
            {(value) => (
              <AccountPicker
                configured={Boolean(settings.seerr)}
                api={props.api}
                id={value.id}
                plexId={value.plexId}
                onChange={(account) => {
                  for (const name of ["seerr_id", "plex_id"] as const) {
                    form.setFieldMeta(name, (meta) => ({
                      ...meta,
                      isTouched: true,
                    }));
                  }

                  form.setFieldMeta("permissions", (meta) => ({
                    ...meta,
                    isTouched: true,
                  }));
                  form.setFieldValue(
                    "seerr_id",
                    account ? String(account.id) : "",
                  );
                  form.setFieldValue(
                    "plex_id",
                    account ? String(account.plex_id) : "",
                  );
                }}
              />
            )}
          </form.Subscribe>
          <form.AppField name="seerr_id">
            {(field) => <field.Errors />}
          </form.AppField>
          <form.AppField name="plex_id">
            {(field) => <field.Errors />}
          </form.AppField>
          <form.AppField name="permissions">
            {(field) => (
              <div className="grid gap-2">
                <field.Control label="Permissions">
                  <Grants
                    sources={settings.sources}
                    value={field.state.value}
                    onChange={(value) => {
                      field.handleChange(value);
                      field.handleBlur();
                    }}
                  />
                </field.Control>
              </div>
            )}
          </form.AppField>
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
