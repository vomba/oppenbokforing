import { pickDirectory } from "./dialogs"

export async function resolveExportDirectory(
  title: string,
  defaultExportDirectory: string | null | undefined,
) {
  if (defaultExportDirectory?.trim()) {
    return defaultExportDirectory
  }
  return pickDirectory(title, defaultExportDirectory)
}
