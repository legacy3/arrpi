import { parseAsString, useQueryState } from "nuqs";

import type { SettingsDocument } from "@/lib/settings";

import { appInfo } from "@/lib/presentation";

import { ConnectionStatus } from "./connection-status";
import { EntryEditor, EntryList, RemoveEntry } from "./entry-list";
import { type EditorProps, useSettingsForm } from "./form";
import { seerrSchema, sourceSchema } from "./schemas";

export function Connections(props: EditorProps) {
  const [entry, setEntry] = useQueryState("entry", parseAsString);
  const selected = entry === "__new" ? "" : entry;
  const setSelected = (value: string | null) =>
    void setEntry(value === "" ? "__new" : value);
  const select = (value: string | null) => {
    props.onEdit();
    setSelected(value);
  };
  const { settings } = props.document;
  const source = selected ? settings.sources[selected] : undefined;

  if (selected === "" || source) {
    return (
      <EntryEditor title={source ? `Edit ${selected}` : "Add source"}>
        <SourceEditor
          key={props.document.revision}
          {...props}
          onSaved={(restart) => {
            props.onSaved(restart);

            if (selected === "") {
              setSelected(null);
            }
          }}
          name={source ? (selected ?? "") : ""}
          source={source}
        />
      </EntryEditor>
    );
  }

  if (selected === "seerr") {
    return (
      <EntryEditor title="Seerr connection">
        <SeerrEditor key={props.document.revision} {...props} />
      </EntryEditor>
    );
  }

  return (
    <EntryList
      message={props.message}
      title="Connections"
      addLabel="Add source"
      onAdd={() => select("")}
      onSelect={select}
      entries={[
        ...Object.entries(settings.sources).map(([id, entry]) => ({
          detail: `${appInfo(entry.kind).label}: ${entry.url}`,
          id,
          name: id,
          status: (
            <ConnectionStatus
              api={props.api}
              source={id}
              configured
              revision={props.document.revision}
            />
          ),
        })),
        {
          detail: settings.seerr?.url ?? "Catalog and requests",
          editLabel: "Edit Seerr",
          id: "seerr",
          name: "seerr",
          status: (
            <ConnectionStatus
              api={props.api}
              source="seerr"
              configured={Boolean(settings.seerr)}
              revision={props.document.revision}
            />
          ),
        },
      ]}
    />
  );
}

function keyPlaceholder(kind: string, storedKind?: string) {
  if (kind === "maintainerr") {
    return "Not required";
  }

  return storedKind && storedKind !== "maintainerr" ? "Unchanged" : "Required";
}

function SeerrEditor(props: EditorProps) {
  const { settings } = props.document;
  const { seerr } = settings;
  const { form, mutation } = useSettingsForm(
    props,
    { api_key: "", language: seerr?.language ?? "en", url: seerr?.url ?? "" },
    seerrSchema(Boolean(seerr)),
    (value) => ({
      ...settings,
      seerr: { ...value, api_key: value.api_key || null },
    }),
  );

  return (
    <>
      <form.AppForm>
        <form.Editor mutation={mutation} message={props.message}>
          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="url">
              {(field) => <field.Text label="Seerr URL" type="url" />}
            </form.AppField>
            <form.AppField name="api_key">
              {(field) => (
                <field.Text
                  label="Seerr API key"
                  type="password"
                  placeholder={seerr ? "Unchanged" : "Required"}
                />
              )}
            </form.AppField>
            <form.AppField name="language">
              {(field) => <field.Text label="Catalog language" />}
            </form.AppField>
          </div>
        </form.Editor>
      </form.AppForm>
      {seerr && (
        <RemoveEntry
          {...props}
          label="Seerr"
          blocked={
            settings.oauth?.enabled
              ? "Disable OAuth in OAuth settings before removing Seerr."
              : undefined
          }
          settings={{ ...settings, seerr: null }}
        />
      )}
    </>
  );
}

function SourceEditor({
  name,
  source,
  ...props
}: {
  name: string;
  source?: SettingsDocument["sources"][string];
} & EditorProps) {
  const { settings } = props.document;
  const dependents = settings.clients
    .filter((client) =>
      client.permissions.some((permission) =>
        permission.startsWith(`${name}:`),
      ),
    )
    .map((client) => client.name);
  const { form, mutation } = useSettingsForm(
    props,
    {
      api_key: "",
      kind: source?.kind ?? "sonarr",
      name,
      seerr_server_id: source?.seerr_server_id?.toString() ?? "",
      url: source?.url ?? "",
    },
    sourceSchema(
      Object.keys(settings.sources).filter((entry) => entry !== name),
      Boolean(source && source.kind !== "maintainerr"),
    ),
    (value) => ({
      ...settings,
      sources: {
        ...settings.sources,
        [value.name]: {
          api_key: value.api_key || (value.kind === "maintainerr" ? "" : null),
          kind: value.kind,
          seerr_server_id:
            ["radarr", "sonarr"].includes(value.kind) && value.seerr_server_id
              ? Number(value.seerr_server_id)
              : null,
          url: value.url,
        },
      },
    }),
  );

  return (
    <>
      <form.AppForm>
        <form.Editor mutation={mutation} message={props.message}>
          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="name">
              {(field) => (
                <field.Text label="Name" readOnly={Boolean(source)} />
              )}
            </form.AppField>
            <form.AppField name="kind">
              {(field) => (
                <field.Select
                  label="App"
                  options={[
                    "plex",
                    "sonarr",
                    "radarr",
                    "sabnzbd",
                    "cleanuparr",
                    "bazarr",
                    "maintainerr",
                    "crosswatch",
                  ]}
                />
              )}
            </form.AppField>
            <form.AppField name="url">
              {(field) => <field.Text label="URL" type="url" />}
            </form.AppField>
            <form.Subscribe selector={(state) => state.values.kind}>
              {(kind) => (
                <form.AppField name="api_key">
                  {(field) => (
                    <field.Text
                      label="API key"
                      type="password"
                      placeholder={keyPlaceholder(kind, source?.kind)}
                    />
                  )}
                </form.AppField>
              )}
            </form.Subscribe>
            <form.Subscribe selector={(state) => state.values.kind}>
              {(kind) =>
                ["radarr", "sonarr"].includes(kind) ? (
                  <form.AppField name="seerr_server_id">
                    {(field) => (
                      <field.Text
                        label="Seerr server ID (profile selection)"
                        type="number"
                        min={0}
                        max={4_294_967_295}
                      />
                    )}
                  </form.AppField>
                ) : null
              }
            </form.Subscribe>
          </div>
        </form.Editor>
      </form.AppForm>
      {source && (
        <RemoveEntry
          {...props}
          label={name}
          blocked={
            dependents.length > 0
              ? `Remove this source's permissions from Access first: ${dependents.join(", ")}.`
              : undefined
          }
          settings={{
            ...settings,
            sources: Object.fromEntries(
              Object.entries(settings.sources).filter(
                ([entry]) => entry !== name,
              ),
            ),
          }}
        />
      )}
    </>
  );
}
