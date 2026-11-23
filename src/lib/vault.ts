/** Metadata returned by the vault list. Secrets are fetched separately. */
export interface VaultItem {
  id: string;
  kind: string;
  name: string;
  username: string;
  website: string;
  last4: string | null;
  created_at: string;
  updated_at: string;
}

/** Metadata-only preview of a password-manager export. */
export interface VaultImportPreviewItem {
  kind: string;
  name: string;
  username: string;
  website: string;
  duplicate: boolean;
}

export interface VaultImportPreview {
  format: string;
  total: number;
  importable: number;
  duplicates: number;
  skipped: number;
  items: VaultImportPreviewItem[];
  more: boolean;
}

export interface VaultImportResult {
  format: string;
  imported: number;
  duplicates: number;
  skipped: number;
  failed: number;
}
