import { Field } from "@/components/field";
import { numericFields } from "@/lib/generated/form-constraints";

import { type EditorProps, useSettingsForm } from "./form";
import { cacheSchema } from "./schemas";

const groups = [
  {
    keys: [
      "fresh_seconds",
      "stale_seconds",
      "retry_seconds",
      "max_entries",
      "max_body_bytes",
      "max_retained_bytes",
    ],
    title: "Cache",
  },
  {
    keys: [
      "timeout_seconds",
      "max_upstream_requests",
      "max_requests",
      "max_normalizations",
    ],
    title: "Requests",
  },
  {
    keys: [
      "max_connections",
      "connection_idle_seconds",
      "connection_lifetime_seconds",
    ],
    title: "Connections",
  },
] as const;
const labels: Record<keyof typeof numericFields.cache, string> = {
  connection_idle_seconds: "Idle timeout (seconds)",
  connection_lifetime_seconds: "Connection lifetime (seconds)",
  fresh_seconds: "Fresh data (seconds)",
  max_body_bytes: "Maximum response (bytes)",
  max_connections: "Maximum connections",
  max_entries: "Cached queries per source",
  max_normalizations: "Concurrent projections",
  max_requests: "Active API responses",
  max_retained_bytes: "Shared cache budget (bytes)",
  max_upstream_requests: "Upstream requests per source",
  retry_seconds: "Retry delay (seconds)",
  stale_seconds: "Stale fallback (seconds)",
  timeout_seconds: "Upstream timeout (seconds)",
};

export function Server(props: EditorProps) {
  const { settings } = props.document;
  const { form, mutation } = useSettingsForm(
    props,
    settings.cache,
    cacheSchema,
    (value) => ({ ...settings, cache: value }),
  );

  return (
    <div className="max-w-3xl">
      <form.AppForm>
        <form.Editor mutation={mutation} message={props.message}>
          {groups.map((group) => (
            <section key={group.title} className="grid gap-4 border-b pb-5">
              <h2 className="text-lg font-semibold">{group.title}</h2>
              <div className="grid gap-4 sm:grid-cols-2">
                {group.keys.map((name) => (
                  <form.AppField key={name} name={name}>
                    {(field) => (
                      <field.Number
                        label={labels[name]}
                        {...numericFields.cache[name]}
                      />
                    )}
                  </form.AppField>
                ))}
              </div>
            </section>
          ))}
          <Field
            label="Bind address"
            name="listen"
            defaultValue={settings.listen}
            readOnly
          />
          <p className="text-xs text-muted-foreground">
            Change the bind address through the CLI. Connection limit changes
            require a restart.
          </p>
        </form.Editor>
      </form.AppForm>
    </div>
  );
}
