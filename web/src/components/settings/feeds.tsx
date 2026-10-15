import { numericFields } from "@/lib/generated/form-constraints";

import { type EditorProps, useSettingsForm } from "./form";
import { feedsSchema } from "./schemas";

export function Feeds(props: EditorProps) {
  const { settings } = props.document;
  const { form, mutation } = useSettingsForm(
    props,
    {
      ...settings.feed,
      enabled: settings.subscriptions.enabled,
      future_days: settings.subscriptions.future_days,
      max_subjects: settings.subscriptions.max_subjects,
      past_days: settings.subscriptions.past_days,
    },
    feedsSchema,
    (value) => ({
      ...settings,
      feed: { link: value.link, title: value.title },
      subscriptions: {
        ...settings.subscriptions,
        enabled: value.enabled,
        future_days: value.future_days,
        max_subjects: value.max_subjects,
        past_days: value.past_days,
      },
    }),
  );

  return (
    <div className="max-w-3xl">
      <form.AppForm>
        <form.Editor mutation={mutation} message={props.message}>
          <h2 className="text-lg font-semibold">Calendar feeds</h2>
          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="title">
              {(field) => <field.Text label="Title" />}
            </form.AppField>
            <form.AppField name="link">
              {(field) => <field.Text label="Site URL" type="url" />}
            </form.AppField>
          </div>
          <form.AppField name="enabled">
            {(field) => (
              <field.Toggle label="Enable personal RSS and iCal subscriptions" />
            )}
          </form.AppField>
          <div className="grid gap-4 sm:grid-cols-3">
            <form.AppField name="past_days">
              {(field) => (
                <field.Number
                  label="Past days"
                  {...numericFields.subscriptions.past_days}
                />
              )}
            </form.AppField>
            <form.AppField name="future_days">
              {(field) => (
                <field.Number
                  label="Future days"
                  {...numericFields.subscriptions.future_days}
                />
              )}
            </form.AppField>
            <form.AppField name="max_subjects">
              {(field) => (
                <field.Number
                  label="Maximum subscribers"
                  {...numericFields.subscriptions.max_subjects}
                />
              )}
            </form.AppField>
          </div>
        </form.Editor>
      </form.AppForm>
    </div>
  );
}
