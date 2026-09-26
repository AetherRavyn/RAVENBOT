/**
 * Display-layer i18n for the static catalogs in $lib/utils (office templates,
 * DiceBear style library). English catalog strings are the fallback, so a
 * missing key still renders correct English instead of a raw key.
 *
 * Policy: persisted fields (rank names, specialties, style `value`s, brand
 * labels like "Avataaars") are DATA and stay raw; only chrome strings shown
 * in pickers/cards go through t(). Same convention as skills.catalog in
 * SkillManager (P7 batch 18).
 */
import { t, type TranslationKey } from "$lib/i18n";
import { OFFICE_TEMPLATES, dicebearStyles } from "$lib/utils";

function lookup(key: string, fallback: string): string {
  const v = t(key as TranslationKey);
  return v === key ? fallback : v;
}

function tmplOf(key: string) {
  return (
    (OFFICE_TEMPLATES as Record<string, (typeof OFFICE_TEMPLATES)[keyof typeof OFFICE_TEMPLATES]>)
      [key] ?? OFFICE_TEMPLATES.custom
  );
}

const styleDescriptions = new Map(
  dicebearStyles().map((s) => [s.value, s.description] as const)
);

export function officeTemplateName(key: string): string {
  return lookup(`office.tmpl.${key}.name`, tmplOf(key).name);
}

export function officeTemplateDesc(key: string): string {
  return lookup(`office.tmpl.${key}.desc`, tmplOf(key).description);
}

export function avatarAllLabel(): string {
  return lookup("avatar.all", "All");
}

export function avatarCategoryLabel(category: string): string {
  const slug = category
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return lookup(`avatar.cat.${slug}`, category);
}

export function avatarStyleDescription(value: string): string {
  return lookup(`avatar.desc.${value}`, styleDescriptions.get(value) ?? "");
}
