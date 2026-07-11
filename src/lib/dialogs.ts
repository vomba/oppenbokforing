import { open, save } from "@tauri-apps/plugin-dialog"

export async function pickDirectory(title: string, defaultPath?: string | null) {
  const selected = await open({
    directory: true,
    multiple: false,
    title,
    defaultPath: defaultPath ?? undefined,
  })
  if (!selected || Array.isArray(selected)) return null
  return selected
}

export async function pickBackupFile(title: string, defaultPath?: string | null) {
  const selected = await open({
    multiple: false,
    title,
    defaultPath: defaultPath ?? undefined,
    filters: [{ name: "Skat backup", extensions: ["skatbackup"] }],
  })
  if (!selected || Array.isArray(selected)) return null
  return selected
}

export async function pickOpenFile(
  title: string,
  options?: {
    defaultPath?: string | null
    filters?: { name: string; extensions: string[] }[]
  },
) {
  const selected = await open({
    multiple: false,
    title,
    defaultPath: options?.defaultPath ?? undefined,
    filters: options?.filters,
  })
  if (!selected || Array.isArray(selected)) return null
  return selected
}

export async function pickSaveBackupFile(title: string, defaultPath?: string | null) {
  const selected = await save({
    title,
    defaultPath: defaultPath ?? undefined,
    filters: [{ name: "Skat backup", extensions: ["skatbackup"] }],
  })
  return selected ?? null
}

export async function pickSaveFile(
  title: string,
  options?: {
    defaultPath?: string | null
    filters?: { name: string; extensions: string[] }[]
  },
) {
  const selected = await save({
    title,
    defaultPath: options?.defaultPath ?? undefined,
    filters: options?.filters,
  })
  return selected ?? null
}

export function fileNameFromPath(path: string) {
  const parts = path.split(/[/\\]/)
  return parts[parts.length - 1] ?? path
}
