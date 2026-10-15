"use client";

import { createFormHook, createFormHookContexts } from "@tanstack/react-form";
import { type ComponentProps, type ReactNode, useEffect, useRef } from "react";
import { type z } from "zod";

import type { Api } from "@/lib/api";
import type { SettingsDocument, SettingsEnvelope } from "@/lib/settings";

import { FormActions } from "@/components/form-actions";
import { Input } from "@/components/ui/input";
import { useConfigMutation } from "@/lib/use-config-mutation";
import { cn } from "@/lib/utils";

export type EditorProps = Readonly<{
  api: Api;
  document: SettingsEnvelope;
  message?: string;
  onEdit: () => void;
  onSaved: (restart: boolean) => void;
}>;

const { fieldContext, formContext, useFieldContext, useFormContext } =
  createFormHookContexts();

function Control({
  children,
  inline = false,
  label,
}: Readonly<{ children: ReactNode; label: string; inline?: boolean }>) {
  const field = useFieldContext<unknown>();
  const errors = field.state.meta.isTouched ? field.state.meta.errors : [];
  const messages = [
    ...new Set(
      errors.map((error: unknown) => {
        if (typeof error === "string") {
          return error;
        }

        return error &&
          typeof error === "object" &&
          "message" in error &&
          typeof error.message === "string"
          ? error.message
          : "Invalid value";
      }),
    ),
  ];

  return (
    <div
      className={
        inline
          ? "flex flex-wrap items-center gap-2"
          : "grid content-start gap-2"
      }
    >
      {inline && children}
      <label htmlFor={field.name} className="text-sm font-medium">
        {label}
      </label>
      {!inline && children}
      {messages.length > 0 && (
        <p
          id={`${field.name}-error`}
          role="alert"
          className={cn("text-xs text-destructive", inline && "basis-full")}
        >
          {messages.join(". ")}
        </p>
      )}
    </div>
  );
}

function Editor({
  children,
  message,
  mutation,
}: Readonly<{
  children: ReactNode;
  mutation: Pick<
    ReturnType<typeof useConfigMutation>,
    "isPending" | "error" | "conflict" | "reload"
  >;
  message?: string;
}>) {
  const form = useFormContext();
  const formRef = useRef<HTMLFormElement>(null);

  useEffect(() => {
    if (message && !mutation.isPending) {
      formRef.current
        ?.querySelector<HTMLButtonElement>('button[type="submit"]')
        ?.focus();
    }
  }, [message, mutation.isPending]);

  return (
    <form
      ref={formRef}
      noValidate
      onSubmit={(event) => {
        event.preventDefault();
        void form.handleSubmit();
      }}
      className="grid gap-5"
    >
      <fieldset disabled={mutation.isPending} className="grid min-w-0 gap-4">
        {children}
      </fieldset>
      <form.Subscribe selector={(state) => state.canSubmit}>
        {(canSubmit) => (
          <FormActions
            pending={mutation.isPending}
            disabled={!canSubmit || mutation.conflict}
            error={mutation.error}
            conflict={mutation.conflict}
            reload={mutation.reload}
            message={message}
          />
        )}
      </form.Subscribe>
    </form>
  );
}

function NumberInput({
  label,
  max,
  min,
}: Readonly<{ label: string; min: number; max: number }>) {
  const field = useFieldContext<number>();

  return (
    <Control label={label}>
      <Input
        type="number"
        id={field.name}
        name={field.name}
        min={min}
        max={max}
        step={1}
        value={Number.isNaN(field.state.value) ? "" : field.state.value}
        onBlur={field.handleBlur}
        onChange={(event) => field.handleChange(event.target.valueAsNumber)}
        aria-invalid={field.state.meta.isTouched && !field.state.meta.isValid}
        aria-describedby={
          field.state.meta.isTouched && !field.state.meta.isValid
            ? `${field.name}-error`
            : undefined
        }
      />
    </Control>
  );
}

function Select({
  label,
  options,
}: Readonly<{ label: string; options: readonly string[] }>) {
  const field = useFieldContext<string>();

  return (
    <Control label={label}>
      <select
        id={field.name}
        name={field.name}
        value={field.state.value}
        onChange={(event) => field.handleChange(event.target.value)}
        onBlur={field.handleBlur}
        className="h-8 rounded-lg border border-input bg-background px-2 text-sm"
        aria-invalid={field.state.meta.isTouched && !field.state.meta.isValid}
        aria-describedby={
          field.state.meta.isTouched && !field.state.meta.isValid
            ? `${field.name}-error`
            : undefined
        }
      >
        {options.map((option) => (
          <option key={option} value={option}>
            {option}
          </option>
        ))}
      </select>
    </Control>
  );
}

function Text({
  label,
  ...props
}: { label: string } & Omit<
  ComponentProps<typeof Input>,
  "name" | "value" | "onChange" | "onBlur"
>) {
  const field = useFieldContext<string>();

  return (
    <Control label={label}>
      <Input
        {...props}
        id={field.name}
        name={field.name}
        autoComplete="off"
        value={field.state.value}
        onBlur={field.handleBlur}
        onChange={(event) => field.handleChange(event.target.value)}
        aria-invalid={field.state.meta.isTouched && !field.state.meta.isValid}
        aria-describedby={
          field.state.meta.isTouched && !field.state.meta.isValid
            ? `${field.name}-error`
            : undefined
        }
      />
    </Control>
  );
}

function Toggle({
  disabled = false,
  label,
}: Readonly<{ label: string; disabled?: boolean }>) {
  const field = useFieldContext<boolean>();

  return (
    <Control label={label} inline>
      <input
        type="checkbox"
        disabled={disabled}
        id={field.name}
        name={field.name}
        checked={field.state.value}
        onBlur={field.handleBlur}
        onChange={(event) => field.handleChange(event.target.checked)}
        aria-invalid={field.state.meta.isTouched && !field.state.meta.isValid}
        aria-describedby={
          field.state.meta.isTouched && !field.state.meta.isValid
            ? `${field.name}-error`
            : undefined
        }
      />
    </Control>
  );
}

const { useAppForm } = createFormHook({
  fieldComponents: { Number: NumberInput, Select, Text, Toggle },
  fieldContext,
  formComponents: { Editor },
  formContext,
});

export function useSettingsForm<T>(
  { api, document, onEdit, onSaved }: EditorProps,
  defaultValues: T,
  schema: z.ZodType<T, T>,
  serialize: (value: T) => SettingsDocument,
) {
  const mutation = useConfigMutation(api, document, serialize, [], onSaved);
  const form = useAppForm({
    defaultValues,
    listeners: { onChange: onEdit },
    onSubmit: ({ value }) => mutation.mutate(schema.parse(value)),
    validators: { onChange: schema, onMount: schema },
  });

  return { form, mutation };
}
