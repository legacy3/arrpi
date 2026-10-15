import { useUpdateEffect } from "ahooks";
import { useRef, useState } from "react";

import type { SettingsDocument } from "@/lib/settings";

import { EntryEditor, EntryList, RemoveEntry } from "./entry-list";
import { type EditorProps, useSettingsForm } from "./form";
import {
  bindingSchema,
  oauthPrerequisite,
  providerSchema,
  requesterClients,
} from "./oauth-schemas";

type Policy = NonNullable<SettingsDocument["oauth"]>;

export function OAuth(props: EditorProps) {
  const [selected, setSelected] = useState<string | null>(null);
  const [savedSection, setSavedSection] = useState<
    "provider" | "identity" | "configuration"
  >("provider");
  const { settings } = props.document;
  const policy = settings.oauth;
  const binding = policy?.bindings.find((entry) => entry.subject === selected);
  const clients = requesterClients(settings);
  const configurationStatusRef = useRef<HTMLParagraphElement>(null);

  if (selected && !binding) {
    setSelected(null);
  }

  useUpdateEffect(() => {
    configurationStatusRef.current?.focus();
  }, [props.message, savedSection]);
  const select = (value: string | null) => {
    props.onEdit();
    setSavedSection("identity");
    setSelected(value);
  };

  if (policy && (selected === "" || binding)) {
    return (
      <EntryEditor
        title={binding ? "Edit OAuth identity" : "Add OAuth identity"}
        onBack={() => select(null)}
      >
        <BindingEditor
          key={props.document.revision}
          {...props}
          policy={policy}
          binding={binding}
          onSaved={(restart) => {
            setSavedSection("identity");
            props.onSaved(restart);
            setSelected(null);
          }}
        />
      </EntryEditor>
    );
  }

  return (
    <div className="grid max-w-3xl gap-6">
      {savedSection === "configuration" && props.message && (
        <p
          ref={configurationStatusRef}
          role="status"
          tabIndex={-1}
          className="text-sm text-primary outline-none"
        >
          {props.message}
        </p>
      )}
      <ProviderEditor
        key={props.document.revision}
        {...props}
        message={savedSection === "provider" ? props.message : undefined}
        onEdit={() => {
          setSavedSection("provider");
          props.onEdit();
        }}
        onSaved={(restart) => {
          setSavedSection("provider");
          props.onSaved(restart);
        }}
      />
      {policy && (
        <>
          <EntryList
            title="OAuth identities"
            message={savedSection === "identity" ? props.message : undefined}
            focusOnMount={savedSection === "identity"}
            addLabel="Add identity"
            addDisabled={policy.bindings.length >= 64 || clients.length === 0}
            onAdd={() => select("")}
            onSelect={select}
            entries={policy.bindings.map((entry) => ({
              detail: entry.client,
              id: entry.subject,
              name: entry.subject,
            }))}
          />
          {policy.bindings.length === 0 && (
            <p className="text-sm text-muted-foreground">
              No identities are bound. OAuth access is denied until you add an
              identity.
            </p>
          )}
          {policy.bindings.length >= 64 && (
            <p className="text-sm text-muted-foreground">
              64 identities configured. Remove an identity to add another.
            </p>
          )}
          {clients.length === 0 && (
            <p className="text-sm text-muted-foreground">
              Add a Seerr-bound client without configuration access in Access
              settings first.
            </p>
          )}
          <RemoveEntry
            key={`remove:${String(props.document.revision)}`}
            {...props}
            focusOnMount={savedSection === "configuration"}
            onEdit={() => {
              setSavedSection("configuration");
              props.onEdit();
            }}
            onSaved={(restart) => {
              setSavedSection("configuration");
              props.onSaved(restart);
            }}
            label="OAuth configuration"
            hint="Removes the provider and all identity bindings. Existing client keys keep working."
            settings={{ ...settings, oauth: null }}
          />
        </>
      )}
    </div>
  );
}

function BindingEditor({
  binding,
  policy,
  ...props
}: { policy: Policy; binding?: Policy["bindings"][number] } & EditorProps) {
  const { settings } = props.document;
  const clients = requesterClients(settings);
  const { form, mutation } = useSettingsForm(
    props,
    {
      client: binding?.client ?? clients[0]?.name ?? "",
      subject: binding?.subject ?? "",
    },
    bindingSchema(settings, binding?.subject),
    (value) => {
      const updated = { ...value, issuer: policy.issuer };

      return {
        ...settings,
        oauth: {
          ...policy,
          bindings: binding
            ? policy.bindings.map((entry) =>
                entry.subject === binding.subject ? updated : entry,
              )
            : [...policy.bindings, updated],
        },
      };
    },
  );

  return (
    <>
      <form.AppForm>
        <form.Editor mutation={mutation} message={props.message}>
          <p className="break-all text-sm text-muted-foreground">
            Provider: {policy.issuer}
          </p>
          <form.AppField name="subject">
            {(field) => (
              <field.Text
                label="Provider subject"
                readOnly={Boolean(binding)}
              />
            )}
          </form.AppField>
          <p className="text-xs text-muted-foreground">
            Copy the provider’s exact sub claim. This is a stable identity, not
            an email address or display name.
          </p>
          <form.AppField name="client">
            {(field) => (
              <field.Select
                label="ArrPi client"
                options={clients.map((client) => client.name)}
              />
            )}
          </form.AppField>
        </form.Editor>
      </form.AppForm>
      {binding && (
        <RemoveEntry
          {...props}
          label="OAuth identity"
          settings={{
            ...settings,
            oauth: {
              ...policy,
              bindings: policy.bindings.filter(
                (entry) => entry.subject !== binding.subject,
              ),
            },
          }}
        />
      )}
    </>
  );
}

function ProviderEditor(props: EditorProps) {
  const { settings } = props.document;
  const policy = settings.oauth;
  const prerequisite = oauthPrerequisite(settings);
  const { form, mutation } = useSettingsForm(
    props,
    {
      enabled: policy?.enabled ?? false,
      issuer: policy?.issuer ?? "",
      required_scope: policy?.required_scope ?? "arrpi:mcp",
    },
    providerSchema(settings),
    (value) => ({
      ...settings,
      oauth: { ...value, bindings: policy?.bindings ?? [] },
    }),
  );

  return (
    <form.AppForm>
      <form.Editor mutation={mutation} message={props.message}>
        <h2 className="font-semibold">OAuth provider</h2>
        <p className="text-sm text-muted-foreground">
          Connect MCP clients through an OAuth provider. It must issue signed
          at+jwt access tokens for the MCP public URL and required scope.
        </p>
        <form.AppField name="enabled">
          {(field) => <field.Toggle label="Enable OAuth" />}
        </form.AppField>
        {prerequisite && (
          <p className="text-sm text-muted-foreground">{prerequisite}</p>
        )}
        <form.AppField name="issuer">
          {(field) => (
            <field.Text
              label="Provider issuer"
              placeholder="https://auth.example.com/application/o/arrpi/"
              readOnly={Boolean(policy?.bindings.length)}
            />
          )}
        </form.AppField>
        {Boolean(policy?.bindings.length) && (
          <p className="text-xs text-muted-foreground">
            Remove identity bindings before changing the provider issuer.
          </p>
        )}
        <form.AppField name="required_scope">
          {(field) => <field.Text label="Required scope" />}
        </form.AppField>
        <p className="break-all text-sm text-muted-foreground">
          Resource and audience:{" "}
          {settings.mcp.public_url || "Set the MCP public URL first."}
        </p>
        <p className="text-xs text-muted-foreground">
          Generic ID tokens and tokens addressed to an OAuth client ID are not
          supported. Each provider subject maps to one existing Seerr-bound
          client.
        </p>
      </form.Editor>
    </form.AppForm>
  );
}
