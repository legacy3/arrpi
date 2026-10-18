import { parseAsArrayOf, parseAsStringLiteral, useQueryState } from "nuqs";

import { Field } from "@/components/field";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { numericFields } from "@/lib/generated/form-constraints";
import {
  policyGroups,
  policyHelp,
  policyLabels,
  policyValue,
} from "@/lib/runtime-policy";

import { type EditorProps, useSettingsForm } from "./form";
import { cacheSchema } from "./schemas";

export function Server(props: EditorProps) {
  const [expanded, setExpanded] = useQueryState(
    "expanded",
    parseAsArrayOf(
      parseAsStringLiteral(policyGroups.map((group) => group.title)),
    ).withDefault([]),
  );
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
          <h2 className="text-lg font-semibold">Advanced server settings</h2>
          <Accordion
            type="multiple"
            value={expanded}
            onValueChange={(value) =>
              void setExpanded(value as typeof expanded)
            }
            className="gap-3"
          >
            {policyGroups.map((group) => (
              <AccordionItem
                key={group.title}
                value={group.title}
                className="rounded-lg border px-4"
              >
                <AccordionTrigger>{group.title}</AccordionTrigger>
                <AccordionContent>
                  <div className="mt-4 grid gap-4 sm:grid-cols-2">
                    {group.keys.map((name) => (
                      <form.AppField key={name} name={name}>
                        {(field) => (
                          <div className="grid gap-1">
                            <field.Number
                              label={policyLabels[name]}
                              {...numericFields.cache[name]}
                            />
                            <p className="text-xs leading-relaxed text-muted-foreground">
                              {policyHelp[name]}
                              {name.endsWith("_bytes") &&
                                ` Current: ${policyValue(name, field.state.value)}.`}
                            </p>
                          </div>
                        )}
                      </form.AppField>
                    ))}
                  </div>
                </AccordionContent>
              </AccordionItem>
            ))}
          </Accordion>
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
