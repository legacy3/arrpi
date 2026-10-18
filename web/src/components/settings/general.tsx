import { type EditorProps, useSettingsForm } from "./form";
import { instanceSchema } from "./schemas";

export function General(props: EditorProps) {
  const { settings } = props.document;
  const { form, mutation } = useSettingsForm(
    props,
    settings.instance,
    instanceSchema,
    (value) => ({ ...settings, instance: value }),
  );

  return (
    <div className="max-w-3xl">
      <form.AppForm>
        <form.Editor mutation={mutation} message={props.message}>
          <h2 className="text-lg font-semibold">Instance identity</h2>
          <form.AppField name="title">
            {(field) => <field.Text label="Title" />}
          </form.AppField>
          <p className="text-sm text-muted-foreground">
            The name of your media stack, shared by ArrPi, MCP and calendar
            feeds.
          </p>
          <form.AppField name="link">
            {(field) => <field.Text label="Public website" type="url" />}
          </form.AppField>
        </form.Editor>
      </form.AppForm>
    </div>
  );
}
